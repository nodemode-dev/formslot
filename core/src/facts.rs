use rusqlite::{Connection, params};

use crate::{dedup, select, symbols};

pub const VERSION: i64 = 1;

fn sql(e: rusqlite::Error) -> String {
    format!("store: {e}")
}

pub fn stale(conn: &Connection, object_format: &str, oid: &str) -> crate::Result<bool> {
    let facts: Option<i64> = conn
        .query_row(
            "SELECT facts FROM blob WHERE object_format = ?1 AND oid = ?2",
            params![object_format, oid],
            |row| row.get(0),
        )
        .map_err(sql)?;
    Ok(facts.unwrap_or(0) < VERSION)
}

pub fn index_blob(conn: &Connection, object_format: &str, oid: &str, path: &str, text: &str) -> crate::Result<()> {
    let language = select::language(path);
    if let Some(found) = symbols::extract(language, path, text) {
        symbols::store(conn, object_format, oid, &found)?;
        let spans: Vec<dedup::Span> = found
            .units
            .iter()
            .map(|u| dedup::Span {
                start_byte: u.start_byte,
                end_byte: u.end_byte,
                kind: u.kind.clone(),
                name: u.name.clone(),
            })
            .collect();
        dedup::store(conn, object_format, oid, &dedup::fingerprint(language, text, &spans))?;
    }
    conn.execute(
        "UPDATE blob SET facts = ?3 WHERE object_format = ?1 AND oid = ?2",
        params![object_format, oid, VERSION],
    )
    .map_err(sql)?;
    Ok(())
}

pub fn link(conn: &Connection, corpus_id: i64) -> crate::Result<()> {
    symbols::link(conn, corpus_id)?;
    dedup::link(conn)?;
    Ok(())
}

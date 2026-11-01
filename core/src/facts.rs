use rusqlite::{Connection, params};

use crate::{dedup, select, symbols};

pub const VERSION: i64 = 1;

fn sql(e: rusqlite::Error) -> String {
    format!("store: {e}")
}

pub fn stale(conn: &Connection, object_format: &str, oid: &str, path: &str) -> crate::Result<bool> {
    let (facts, family): (Option<i64>, Option<String>) = conn
        .query_row(
            "SELECT facts, facts_family FROM blob WHERE object_format = ?1 AND oid = ?2",
            params![object_format, oid],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(sql)?;
    if facts.unwrap_or(0) < VERSION {
        return Ok(true);
    }
    Ok(family.is_none() && symbols::family(select::language(path)).is_some())
}

pub fn index_blob(conn: &Connection, object_format: &str, oid: &str, path: &str, text: &str) -> crate::Result<()> {
    let language = select::language(path);
    let family = symbols::family(language);
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
        "UPDATE blob SET facts = ?3, facts_family = ?4 WHERE object_format = ?1 AND oid = ?2",
        params![object_format, oid, VERSION, family],
    )
    .map_err(sql)?;
    Ok(())
}

pub fn link(conn: &Connection, corpus_id: i64) -> crate::Result<()> {
    symbols::link(conn, corpus_id)?;
    dedup::link(conn)?;
    Ok(())
}

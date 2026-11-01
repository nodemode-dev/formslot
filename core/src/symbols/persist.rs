use rusqlite::{Connection, params};

use super::{Extracted, sig8};

fn sql(e: rusqlite::Error) -> String {
    format!("symbols: {e}")
}

pub fn store(conn: &Connection, object_format: &str, oid: &str, x: &Extracted) -> crate::Result<()> {
    conn.execute_batch("SAVEPOINT symbols_store").map_err(sql)?;
    match write(conn, object_format, oid, x) {
        Ok(()) => conn.execute_batch("RELEASE symbols_store").map_err(sql),
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK TO symbols_store; RELEASE symbols_store");
            Err(e)
        }
    }
}

fn write(conn: &Connection, object_format: &str, oid: &str, x: &Extracted) -> crate::Result<()> {
    for table in ["unit", "unit_ref", "unit_import"] {
        conn.execute(
            &format!("DELETE FROM {table} WHERE object_format = ?1 AND oid = ?2"),
            params![object_format, oid],
        )
        .map_err(sql)?;
    }
    let mut unit = conn
        .prepare_cached(
            "INSERT INTO unit (object_format, oid, idx, kind, name, qualified, signature, sig8, exported, \
             start_byte, end_byte, start_line, end_line, parent_idx) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        )
        .map_err(sql)?;
    for (idx, u) in x.units.iter().enumerate() {
        unit.execute(params![
            object_format,
            oid,
            idx as i64,
            u.kind,
            u.name,
            u.qualified,
            u.signature,
            sig8(&u.signature),
            u.exported as i64,
            u.start_byte as i64,
            u.end_byte as i64,
            u.start_line as i64,
            u.end_line as i64,
            u.parent.map(|p| p as i64),
        ])
        .map_err(sql)?;
    }
    let mut reference = conn
        .prepare_cached(
            "INSERT INTO unit_ref (object_format, oid, name, kind, line) VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .map_err(sql)?;
    for r in &x.refs {
        reference
            .execute(params![object_format, oid, r.name, r.kind, r.line as i64])
            .map_err(sql)?;
    }
    let mut import = conn
        .prepare_cached(
            "INSERT INTO unit_import (object_format, oid, source, names, line) VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .map_err(sql)?;
    for i in &x.imports {
        import
            .execute(params![object_format, oid, i.source, i.names.join(","), i.line as i64])
            .map_err(sql)?;
    }
    conn.execute(
        "UPDATE blob SET facts = 1 WHERE object_format = ?1 AND oid = ?2",
        params![object_format, oid],
    )
    .map_err(sql)?;
    Ok(())
}

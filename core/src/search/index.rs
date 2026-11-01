use rusqlite::{Connection, OptionalExtension, params};

use super::text::split_words;
use crate::git::{BlobReader, Repo};

const BODY_CAP: usize = 8192;
const CONTAINER_BODY_CAP: usize = 1536;
const CONTAINER_KINDS: [&str; 6] = ["class", "namespace", "mod", "struct", "trait", "interface"];

#[derive(Debug, Clone, Default)]
pub struct SyncStats {
    pub files: u64,
    pub added: u64,
    pub removed: u64,
}

fn sql(e: rusqlite::Error) -> String {
    format!("search: {e}")
}

fn cut(text: &str, cap: usize) -> &str {
    if text.len() <= cap {
        return text;
    }
    let mut end = cap;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

struct UnitRow {
    idx: i64,
    kind: String,
    name: String,
    qualified: String,
    signature: String,
    exported: i64,
    start_byte: i64,
    end_byte: i64,
    start_line: i64,
    end_line: i64,
}

fn units_of(conn: &Connection, format: &str, oid: &str) -> crate::Result<Vec<UnitRow>> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT idx, kind, name, qualified, signature, exported, start_byte, end_byte, start_line, end_line \
             FROM unit WHERE object_format = ?1 AND oid = ?2 ORDER BY idx",
        )
        .map_err(sql)?;
    let rows = stmt
        .query_map(params![format, oid], |r| {
            Ok(UnitRow {
                idx: r.get(0)?,
                kind: r.get(1)?,
                name: r.get(2)?,
                qualified: r.get(3)?,
                signature: r.get(4)?,
                exported: r.get(5)?,
                start_byte: r.get(6)?,
                end_byte: r.get(7)?,
                start_line: r.get(8)?,
                end_line: r.get(9)?,
            })
        })
        .map_err(sql)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(sql)
}

fn add_file(
    conn: &Connection,
    corpus_id: i64,
    format: &str,
    path: &str,
    oid: &str,
    data: Option<&[u8]>,
) -> crate::Result<u64> {
    let units = units_of(conn, format, oid)?;
    let mut added = 0;
    for u in &units {
        let body = match data {
            Some(bytes) => {
                let start = (u.start_byte.max(0) as usize).min(bytes.len());
                let end = (u.end_byte.max(0) as usize).min(bytes.len()).max(start);
                let cap = if CONTAINER_KINDS.contains(&u.kind.as_str()) {
                    CONTAINER_BODY_CAP
                } else {
                    BODY_CAP
                };
                let slice = String::from_utf8_lossy(&bytes[start..end]).to_string();
                cut(&slice, cap).to_string()
            }
            None => String::new(),
        };
        conn.prepare_cached(
            "INSERT INTO search_unit (corpus_id, path, oid, idx, kind, name, qualified, signature, exported, \
             start_byte, end_byte, start_line, end_line) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        )
        .map_err(sql)?
        .execute(params![
            corpus_id,
            path,
            oid,
            u.idx,
            u.kind,
            u.name,
            u.qualified,
            u.signature,
            u.exported,
            u.start_byte,
            u.end_byte,
            u.start_line,
            u.end_line
        ])
        .map_err(sql)?;
        let id = conn.last_insert_rowid();
        let words = split_words(&u.qualified).join(" ");
        conn.prepare_cached("INSERT INTO unit_fts (rowid, name, words, signature, body) VALUES (?1, ?2, ?3, ?4, ?5)")
            .map_err(sql)?
            .execute(params![id, u.name, words, u.signature, body])
            .map_err(sql)?;
        conn.prepare_cached("INSERT INTO unit_tri (rowid, name, qualified, path) VALUES (?1, ?2, ?3, ?4)")
            .map_err(sql)?
            .execute(params![id, u.name, u.qualified, path])
            .map_err(sql)?;
        added += 1;
    }
    Ok(added)
}

pub fn sync(conn: &Connection, corpus_id: i64, format: &str, repo: &Repo) -> crate::Result<SyncStats> {
    let mut stats = SyncStats::default();
    stats.removed = conn
        .execute(
            "DELETE FROM search_unit WHERE corpus_id = ?1 AND NOT EXISTS \
             (SELECT 1 FROM file f WHERE f.corpus_id = search_unit.corpus_id AND f.path = search_unit.path AND f.oid = search_unit.oid)",
            params![corpus_id],
        )
        .map_err(sql)? as u64;
    let pending: Vec<(String, String)> = {
        let mut stmt = conn
            .prepare(
                "SELECT f.path, f.oid FROM file f WHERE f.corpus_id = ?1 \
                 AND EXISTS (SELECT 1 FROM unit u WHERE u.object_format = ?2 AND u.oid = f.oid) \
                 AND NOT EXISTS (SELECT 1 FROM search_unit s WHERE s.corpus_id = f.corpus_id AND s.path = f.path AND s.oid = f.oid) \
                 ORDER BY f.path",
            )
            .map_err(sql)?;
        stmt.query_map(params![corpus_id, format], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(sql)?
            .collect::<Result<_, _>>()
            .map_err(sql)?
    };
    if pending.is_empty() {
        return Ok(stats);
    }
    let mut reader: Option<BlobReader> = None;
    for (path, oid) in &pending {
        if reader.is_none() {
            reader = Some(repo.blob_reader()?);
        }
        let data = match reader.as_mut() {
            Some(r) => r.read(oid).unwrap_or(None),
            None => None,
        };
        stats.added += add_file(conn, corpus_id, format, path, oid, data.as_deref())?;
        stats.files += 1;
    }
    Ok(stats)
}

pub fn indexed(conn: &Connection, corpus_id: i64) -> crate::Result<bool> {
    let found: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM search_unit WHERE corpus_id = ?1 LIMIT 1",
            params![corpus_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(sql)?;
    Ok(found.is_some())
}

pub fn ensure(conn: &Connection, corpus_id: i64, format: &str, path: &str) -> crate::Result<Option<SyncStats>> {
    if indexed(conn, corpus_id)? {
        return Ok(None);
    }
    let repo = Repo::open(std::path::Path::new(path))?;
    let tx = conn.unchecked_transaction().map_err(sql)?;
    let stats = sync(&tx, corpus_id, format, &repo)?;
    tx.commit().map_err(sql)?;
    Ok(Some(stats))
}

pub fn rebuild(conn: &Connection, corpus_id: i64, format: &str, path: &str) -> crate::Result<SyncStats> {
    let repo = Repo::open(std::path::Path::new(path))?;
    let tx = conn.unchecked_transaction().map_err(sql)?;
    tx.execute("DELETE FROM search_unit WHERE corpus_id = ?1", params![corpus_id])
        .map_err(sql)?;
    let stats = sync(&tx, corpus_id, format, &repo)?;
    tx.commit().map_err(sql)?;
    Ok(stats)
}

pub fn rebuild_memory(conn: &Connection) -> crate::Result<()> {
    conn.execute_batch("INSERT INTO memory_fts (memory_fts) VALUES ('rebuild')")
        .map_err(sql)
}

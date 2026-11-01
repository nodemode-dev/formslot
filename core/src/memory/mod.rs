use rusqlite::{Connection, Row, params};
use serde::Serialize;
use sha2::{Digest, Sha256};

pub mod anchors;
pub mod declines;
pub mod hashout;
pub mod ingest;
pub mod markdown;
pub mod propose;
pub mod recall;

pub use anchors::{AnchorCheck, check_anchors};
pub use ingest::{IngestReport, after_index, ingest};

pub const KINDS: [&str; 6] = ["rule", "decision", "how-to", "declined", "hashout", "tombstone"];
pub const STATUSES: [&str; 4] = ["active", "stale", "superseded", "proposed"];

pub fn sql(e: rusqlite::Error) -> String {
    format!("memory: {e}")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn digest(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0u8]);
    }
    hex(&hasher.finalize())
}

pub fn content_hash(kind: &str, authority: &str, anchor: Option<&str>, title: &str, text: &str) -> String {
    digest(&[kind, authority, anchor.unwrap_or(""), title, text])
}

pub fn estimate_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4)
}

#[derive(Debug, Clone, Serialize)]
pub struct Record {
    pub id: i64,
    pub kind: String,
    pub authority: String,
    pub status: String,
    pub title: String,
    pub text: String,
    pub anchor: Option<String>,
    pub corpus_id: Option<i64>,
    pub source: String,
    pub triggers: String,
    pub content_hash: String,
    pub valid_from: String,
    pub valid_to: Option<String>,
    pub superseded_by: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

pub const COLUMNS: &str = "id, kind, authority, status, title, text, anchor, corpus_id, source, triggers, \
     content_hash, valid_from, valid_to, superseded_by, created_at, updated_at";

pub fn from_row(row: &Row) -> rusqlite::Result<Record> {
    Ok(Record {
        id: row.get(0)?,
        kind: row.get(1)?,
        authority: row.get(2)?,
        status: row.get(3)?,
        title: row.get(4)?,
        text: row.get(5)?,
        anchor: row.get(6)?,
        corpus_id: row.get(7)?,
        source: row.get(8)?,
        triggers: row.get(9)?,
        content_hash: row.get(10)?,
        valid_from: row.get(11)?,
        valid_to: row.get(12)?,
        superseded_by: row.get(13)?,
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
    })
}

#[derive(Debug, Clone)]
pub struct NewRecord {
    pub kind: String,
    pub authority: String,
    pub status: String,
    pub title: String,
    pub text: String,
    pub anchor: Option<String>,
    pub corpus_id: Option<i64>,
    pub source: String,
    pub triggers: String,
    pub valid_from: String,
}

pub fn insert(conn: &Connection, new: &NewRecord, now: &str) -> crate::Result<i64> {
    let hash = content_hash(&new.kind, &new.authority, new.anchor.as_deref(), &new.title, &new.text);
    conn.execute(
        "INSERT INTO memory_record (kind, authority, status, title, text, anchor, corpus_id, source, triggers, \
         content_hash, valid_from, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)",
        params![
            new.kind,
            new.authority,
            new.status,
            new.title,
            new.text,
            new.anchor,
            new.corpus_id,
            new.source,
            new.triggers,
            hash,
            new.valid_from,
            now
        ],
    )
    .map_err(sql)?;
    Ok(conn.last_insert_rowid())
}

pub fn event(conn: &Connection, record_id: i64, action: &str, actor: &str, note: &str) -> crate::Result<()> {
    conn.execute(
        "INSERT INTO memory_event (record_id, at, action, actor, note) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![record_id, crate::corpus::now_iso(), action, actor, note],
    )
    .map_err(sql)?;
    Ok(())
}

#[derive(Debug, Clone, Default)]
pub struct Filter {
    pub kind: Option<String>,
    pub status: Option<String>,
    pub corpus_id: Option<i64>,
    pub anchor: Option<String>,
    pub include_superseded: bool,
    pub limit: Option<usize>,
}

pub fn list(conn: &Connection, filter: &Filter) -> crate::Result<Vec<Record>> {
    let mut clauses: Vec<String> = Vec::new();
    let mut values: Vec<rusqlite::types::Value> = Vec::new();
    if let Some(kind) = &filter.kind {
        values.push(kind.clone().into());
        clauses.push(format!("kind = ?{}", values.len()));
    }
    if let Some(status) = &filter.status {
        values.push(status.clone().into());
        clauses.push(format!("status = ?{}", values.len()));
    } else if !filter.include_superseded {
        clauses.push("status != 'superseded'".to_string());
    }
    if let Some(corpus) = filter.corpus_id {
        values.push(corpus.into());
        clauses.push(format!("(corpus_id = ?{} OR corpus_id IS NULL)", values.len()));
    }
    if let Some(anchor) = &filter.anchor {
        values.push(anchor.clone().into());
        clauses.push(format!("anchor = ?{}", values.len()));
    }
    let wher = if clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", clauses.join(" AND "))
    };
    let limit = filter.limit.map(|n| format!(" LIMIT {n}")).unwrap_or_default();
    let query = format!("SELECT {COLUMNS} FROM memory_record{wher} ORDER BY id{limit}");
    let mut stmt = conn.prepare(&query).map_err(sql)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(values.iter()), from_row)
        .map_err(sql)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(sql)?);
    }
    Ok(out)
}

pub fn counts(conn: &Connection, corpus_id: Option<i64>) -> crate::Result<Vec<(String, String, i64)>> {
    let mut stmt = conn
        .prepare(
            "SELECT kind, status, COUNT(*) FROM memory_record \
             WHERE (?1 IS NULL OR corpus_id = ?1) GROUP BY kind, status ORDER BY kind, status",
        )
        .map_err(sql)?;
    let rows = stmt
        .query_map(params![corpus_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(sql)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(sql)?);
    }
    Ok(out)
}

pub fn corpus_of(conn: &Connection, key: &str) -> crate::Result<(i64, String)> {
    let found = conn.query_row(
        "SELECT id, name FROM corpus WHERE name = ?1 OR CAST(id AS TEXT) = ?1 ORDER BY (name = ?1) DESC LIMIT 1",
        params![key],
        |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
    );
    match found {
        Ok(pair) => Ok(pair),
        Err(rusqlite::Error::QueryReturnedNoRows) => Err(format!("no corpus named {key}")),
        Err(e) => Err(sql(e)),
    }
}

pub fn words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            cur.extend(ch.to_lowercase());
        } else if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

pub fn jaccard(a: &str, b: &str) -> f64 {
    let left: std::collections::HashSet<String> = words(a).into_iter().collect();
    let right: std::collections::HashSet<String> = words(b).into_iter().collect();
    if left.is_empty() && right.is_empty() {
        return 1.0;
    }
    let both = left.intersection(&right).count() as f64;
    let either = left.union(&right).count() as f64;
    both / either
}

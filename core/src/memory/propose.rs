use rusqlite::{Connection, params};
use serde::Serialize;

use super::markdown::clip_words;
use super::{COLUMNS, NewRecord, Record, event, from_row, insert, jaccard, sql, words};

const NEAR: f64 = 0.8;
const MAX_TEXT: usize = 8000;

#[derive(Debug, Clone, Serialize)]
pub struct Proposal {
    pub id: i64,
    pub action: String,
    pub target: Option<i64>,
    pub target_kind: Option<String>,
    pub target_authority: Option<String>,
    pub target_status: Option<String>,
    pub note: String,
    pub redacted: usize,
}

fn normal(text: &str) -> String {
    words(text).join(" ")
}

pub fn propose(
    conn: &Connection,
    raw: &str,
    anchor: Option<&str>,
    kind: &str,
    corpus_id: Option<i64>,
) -> crate::Result<Proposal> {
    if kind != "decision" && kind != "how-to" {
        return Err("a proposal is a decision or a how-to".to_string());
    }
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("a proposal needs text".to_string());
    }
    if trimmed.chars().count() > MAX_TEXT {
        return Err(format!("a proposal is at most {MAX_TEXT} characters"));
    }
    let anchor = anchor.map(str::trim).filter(|a| !a.is_empty());
    let (text, redacted) = crate::redact::text(trimmed);
    let wanted = normal(&text);
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {COLUMNS} FROM memory_record WHERE status IN ('active', 'proposed') ORDER BY id DESC"
        ))
        .map_err(sql)?;
    let rows = stmt.query_map([], from_row).map_err(sql)?;
    let mut existing: Vec<Record> = Vec::new();
    for row in rows {
        existing.push(row.map_err(sql)?);
    }
    let same_anchor = |r: &Record| r.anchor.as_deref() == anchor;
    let mut action = "ADD";
    let mut target: Option<&Record> = None;
    let mut note = String::from("nothing recorded matches; the proposal is a new record");
    if let Some(hit) = existing.iter().find(|r| normal(&r.text) == wanted) {
        action = "NOOP";
        target = Some(hit);
        note = format!("record {} already holds this text", hit.id);
    } else {
        let mut best: Option<(&Record, f64)> = None;
        for r in existing.iter().filter(|r| same_anchor(r) && r.kind == kind) {
            let score = jaccard(&r.text, &text);
            if score >= NEAR && best.is_none_or(|(_, s)| score > s) {
                best = Some((r, score));
            }
        }
        if let Some((hit, score)) = best {
            action = "MERGE";
            target = Some(hit);
            note = format!(
                "record {} has the same anchor and kind and {:.0} percent of the same words; extend it instead of adding",
                hit.id,
                score * 100.0
            );
        } else if anchor.is_some()
            && let Some(hit) = existing.iter().find(|r| same_anchor(r) && r.kind == kind)
        {
            action = "SUPERSEDE";
            target = Some(hit);
            note = if hit.authority == "owner" {
                format!(
                    "record {} is an owner record at this anchor and says something different; only the owner supersedes it, and it was not touched",
                    hit.id
                )
            } else {
                format!(
                    "record {} is at the same anchor and kind and says something different; if accepted it keeps its dates and points to the new record",
                    hit.id
                )
            };
        }
    }
    let target_id = target.map(|r| r.id);
    let facts = (
        target.map(|r| r.kind.clone()),
        target.map(|r| r.authority.clone()),
        target.map(|r| r.status.clone()),
    );
    let now = crate::corpus::now_iso();
    let record = NewRecord {
        kind: kind.to_string(),
        authority: "agent".to_string(),
        status: "proposed".to_string(),
        title: clip_words(&text, 12, 90),
        text,
        anchor: anchor.map(str::to_string),
        corpus_id,
        source: "proposal".to_string(),
        triggers: String::new(),
        valid_from: now.clone(),
    };
    let id = insert(conn, &record, &now)?;
    let label = match target_id {
        Some(t) => format!("{action} target={t}: {note}"),
        None => format!("{action}: {note}"),
    };
    event(conn, id, "proposed", "agent", &label)?;
    conn.execute("UPDATE memory_record SET updated_at = ?1 WHERE id = ?2", params![now, id])
        .map_err(sql)?;
    Ok(Proposal {
        id,
        action: action.to_string(),
        target: target_id,
        target_kind: facts.0,
        target_authority: facts.1,
        target_status: facts.2,
        note,
        redacted,
    })
}

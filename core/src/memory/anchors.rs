use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use super::{event, sql};

#[derive(Debug, Clone, Default, Serialize)]
pub struct AnchorCheck {
    pub checked: usize,
    pub anchored: usize,
    pub stale: usize,
    pub restored: usize,
}

fn norm(symbol: &str) -> String {
    let mut text = symbol.trim().to_string();
    if let Some(at) = text.find(" (") {
        text.truncate(at);
    }
    text.trim_end_matches("()").replace("::", ".")
}

pub fn symbol_matches(wanted: &str, qualified: &str, name: &str) -> bool {
    let want = norm(wanted);
    let have = norm(qualified);
    if want == have {
        return true;
    }
    if !want.contains('.') && !want.contains('/') && want == name {
        return true;
    }
    false
}

struct Resolver<'a> {
    conn: &'a Connection,
    corpus_id: i64,
    format: String,
    cache: HashMap<String, Option<Vec<(String, String)>>>,
}

impl<'a> Resolver<'a> {
    fn new(conn: &'a Connection, corpus_id: i64) -> crate::Result<Resolver<'a>> {
        let format: String = conn
            .query_row("SELECT object_format FROM corpus WHERE id = ?1", params![corpus_id], |r| r.get(0))
            .map_err(sql)?;
        Ok(Resolver { conn, corpus_id, format, cache: HashMap::new() })
    }

    fn units_of(&mut self, path: &str) -> crate::Result<&Option<Vec<(String, String)>>> {
        if !self.cache.contains_key(path) {
            let oid: Option<String> = self
                .conn
                .query_row(
                    "SELECT oid FROM file WHERE corpus_id = ?1 AND path = ?2",
                    params![self.corpus_id, path],
                    |r| r.get(0),
                )
                .optional()
                .map_err(sql)?;
            let value = match oid {
                None => None,
                Some(oid) => {
                    let mut stmt = self
                        .conn
                        .prepare_cached("SELECT qualified, name FROM unit WHERE object_format = ?1 AND oid = ?2")
                        .map_err(sql)?;
                    let rows = stmt
                        .query_map(params![self.format, oid], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                        .map_err(sql)?;
                    let mut list = Vec::new();
                    for row in rows {
                        list.push(row.map_err(sql)?);
                    }
                    Some(list)
                }
            };
            self.cache.insert(path.to_string(), value);
        }
        Ok(&self.cache[path])
    }

    fn resolves(&mut self, anchor: &str) -> crate::Result<bool> {
        let (path, symbol) = match anchor.split_once('#') {
            Some((p, s)) => (p, Some(s)),
            None => (anchor, None),
        };
        let units = self.units_of(path)?;
        let Some(list) = units else { return Ok(false) };
        match symbol {
            None => Ok(true),
            Some(symbol) => Ok(list.iter().any(|(q, n)| symbol_matches(symbol, q, n))),
        }
    }
}

pub fn check_anchors(conn: &Connection, corpus_id: i64) -> crate::Result<AnchorCheck> {
    let mut resolver = Resolver::new(conn, corpus_id)?;
    let mut rows: Vec<(i64, String, String)> = Vec::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT id, anchor, status FROM memory_record \
                 WHERE corpus_id = ?1 AND anchor IS NOT NULL AND status IN ('active', 'stale') ORDER BY id",
            )
            .map_err(sql)?;
        let mapped = stmt
            .query_map(params![corpus_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(sql)?;
        for row in mapped {
            rows.push(row.map_err(sql)?);
        }
    }
    let now = crate::corpus::now_iso();
    let mut report = AnchorCheck { checked: rows.len(), ..AnchorCheck::default() };
    let mut seen = conn
        .prepare_cached("SELECT EXISTS(SELECT 1 FROM memory_event WHERE record_id = ?1 AND action = 'anchored')")
        .map_err(sql)?;
    let mut last = conn
        .prepare_cached("SELECT action, note FROM memory_event WHERE record_id = ?1 ORDER BY id DESC LIMIT 1")
        .map_err(sql)?;
    for (id, anchor, status) in rows {
        let resolved = resolver.resolves(&anchor)?;
        let was_anchored: bool = seen.query_row(params![id], |r| r.get(0)).map_err(sql)?;
        if resolved {
            if !was_anchored {
                event(conn, id, "anchored", "anchors", &format!("{anchor} resolves to a unit"))?;
                report.anchored += 1;
            }
            if status == "stale" {
                let latest: Option<(String, String)> = last
                    .query_row(params![id], |r| Ok((r.get(0)?, r.get(1)?)))
                    .optional()
                    .map_err(sql)?;
                if let Some((action, note)) = latest
                    && action == "stale"
                    && note.starts_with("anchor ")
                {
                    conn.execute(
                        "UPDATE memory_record SET status = 'active', updated_at = ?1 WHERE id = ?2",
                        params![now, id],
                    )
                    .map_err(sql)?;
                    event(conn, id, "restored", "anchors", &format!("anchor {anchor} resolves again"))?;
                    report.restored += 1;
                }
            }
        } else if was_anchored && status == "active" {
            conn.execute(
                "UPDATE memory_record SET status = 'stale', updated_at = ?1 WHERE id = ?2",
                params![now, id],
            )
            .map_err(sql)?;
            event(conn, id, "stale", "anchors", &format!("anchor {anchor} no longer resolves to a unit"))?;
            report.stale += 1;
        }
    }
    Ok(report)
}

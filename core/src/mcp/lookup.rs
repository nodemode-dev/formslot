use std::collections::HashMap;
use std::path::Path;

use rusqlite::{Connection, params};
use serde_json::{Value, json};

use crate::git::{BlobReader, Repo};
use crate::redact;
use crate::search::pack::{Excerpt, Source, copy_verdict};
use crate::search::{Hit, Target};

fn sql(e: rusqlite::Error) -> String {
    format!("mcp: {e}")
}

fn like(text: &str) -> String {
    text.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

pub fn corpus_names(conn: &Connection) -> Result<HashMap<i64, String>, String> {
    let mut stmt = conn.prepare("SELECT id, name FROM corpus").map_err(sql)?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .map_err(sql)?;
    let mut out = HashMap::new();
    for row in rows {
        let (id, name) = row.map_err(sql)?;
        out.insert(id, name);
    }
    Ok(out)
}

pub fn references(conn: &Connection, corpora: &[Target], symbol: &str) -> Result<Vec<Value>, String> {
    let mut out = Vec::new();
    for target in corpora {
        let mut stmt;
        let rows: Vec<Value> = if let Some((path, qualified)) = symbol.split_once('#') {
            stmt = conn
                .prepare(
                    "SELECT from_path, from_qualified, line, kind, to_path, to_qualified FROM edge \
                     WHERE corpus_id = ?1 AND to_path = ?2 AND to_qualified = ?3 ORDER BY from_path, line",
                )
                .map_err(sql)?;
            collect_edges(&mut stmt, params![target.id, path, qualified], &target.name)?
        } else {
            let escaped = like(symbol);
            stmt = conn
                .prepare(
                    "SELECT from_path, from_qualified, line, kind, to_path, to_qualified FROM edge \
                     WHERE corpus_id = ?1 AND (to_qualified = ?2 OR to_qualified LIKE ?3 ESCAPE '\\' OR to_qualified LIKE ?4 ESCAPE '\\') \
                     ORDER BY from_path, line",
                )
                .map_err(sql)?;
            collect_edges(
                &mut stmt,
                params![target.id, symbol, format!("%.{escaped}"), format!("%::{escaped}")],
                &target.name,
            )?
        };
        out.extend(rows);
    }
    Ok(out)
}

fn collect_edges(stmt: &mut rusqlite::Statement, args: impl rusqlite::Params, repo: &str) -> Result<Vec<Value>, String> {
    let rows = stmt
        .query_map(args, |r| {
            Ok(json!({
                "type": "reference",
                "repo": repo,
                "path": r.get::<_, String>(0)?,
                "qualified": r.get::<_, String>(1)?,
                "line": r.get::<_, i64>(2)?,
                "kind": r.get::<_, String>(3)?,
                "to": { "path": r.get::<_, String>(4)?, "qualified": r.get::<_, String>(5)? },
            }))
        })
        .map_err(sql)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(sql)?);
    }
    Ok(out)
}

pub fn slot_references(conn: &Connection, corpus: &Target, paths: &[String]) -> Result<Vec<Value>, String> {
    let mut out = Vec::new();
    let mut stmt = conn
        .prepare(
            "SELECT from_path, from_qualified, line, kind, to_path, to_qualified FROM edge \
             WHERE corpus_id = ?1 AND to_path = ?2 ORDER BY from_path, line",
        )
        .map_err(sql)?;
    for path in paths {
        let rows = collect_edges(&mut stmt, params![corpus.id, path], &corpus.name)?;
        out.extend(rows.into_iter().filter(|r| !paths.iter().any(|p| r["path"].as_str() == Some(p.as_str()))));
    }
    Ok(out)
}

#[derive(Debug, Clone)]
struct Near {
    corpus: i64,
    path: String,
    qualified: String,
    start: i64,
    end: i64,
    relation: String,
    score: f64,
}

fn rank(relation: &str) -> u8 {
    match relation {
        "clone-T1" => 0,
        "clone-T2" => 1,
        "clone-T3" => 2,
        "related-similar" => 3,
        _ => 4,
    }
}

fn keep(found: &mut HashMap<(i64, String, String, i64), Near>, near: Near) {
    let key = (near.corpus, near.path.clone(), near.qualified.clone(), near.start);
    let better = match found.get(&key) {
        Some(old) => rank(&near.relation) < rank(&old.relation) || (rank(&near.relation) == rank(&old.relation) && near.score > old.score),
        None => true,
    };
    if better {
        found.insert(key, near);
    }
}

pub fn near_unit(conn: &Connection, corpus: &Target, path: &str, qualified: &str) -> Result<Vec<Value>, String> {
    let mut found: HashMap<(i64, String, String, i64), Near> = HashMap::new();
    let mut groups = conn
        .prepare(
            "SELECT g.tier, m2.corpus_id, m2.path, m2.qualified, m2.start_line, m2.end_line \
             FROM clone_member m1 JOIN clone_group g ON g.id = m1.group_id \
             JOIN clone_member m2 ON m2.group_id = m1.group_id \
             WHERE m1.corpus_id = ?1 AND m1.path = ?2 AND m1.qualified = ?3 \
             AND NOT (m2.corpus_id = m1.corpus_id AND m2.path = m1.path AND m2.qualified = m1.qualified AND m2.start_line = m1.start_line)",
        )
        .map_err(sql)?;
    let rows = groups
        .query_map(params![corpus.id, path, qualified], |r| {
            Ok(Near {
                relation: format!("clone-{}", r.get::<_, String>(0)?),
                corpus: r.get(1)?,
                path: r.get(2)?,
                qualified: r.get(3)?,
                start: r.get(4)?,
                end: r.get(5)?,
                score: 1.0,
            })
        })
        .map_err(sql)?;
    for row in rows {
        keep(&mut found, row.map_err(sql)?);
    }
    let sides = [
        ("clone_pair", "tier", "clone-", "a", "b"),
        ("clone_pair", "tier", "clone-", "b", "a"),
        ("related_pair", "reason", "related-", "a", "b"),
        ("related_pair", "reason", "related-", "b", "a"),
    ];
    for (table, label, prefix, mine, other) in sides {
        let query = format!(
            "SELECT {label}, score, {other}_corpus, {other}_path, {other}_qualified, {other}_start, {other}_end FROM {table} \
             WHERE {mine}_corpus = ?1 AND {mine}_path = ?2 AND {mine}_qualified = ?3"
        );
        let mut stmt = conn.prepare(&query).map_err(sql)?;
        let rows = stmt
            .query_map(params![corpus.id, path, qualified], |r| {
                Ok(Near {
                    relation: format!("{prefix}{}", r.get::<_, String>(0)?),
                    score: r.get(1)?,
                    corpus: r.get(2)?,
                    path: r.get(3)?,
                    qualified: r.get(4)?,
                    start: r.get(5)?,
                    end: r.get(6)?,
                })
            })
            .map_err(sql)?;
        for row in rows {
            keep(&mut found, row.map_err(sql)?);
        }
    }
    let names = corpus_names(conn)?;
    let mut list: Vec<Near> = found.into_values().collect();
    list.sort_by(|a, b| {
        rank(&a.relation)
            .cmp(&rank(&b.relation))
            .then(b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal))
            .then(names.get(&a.corpus).cmp(&names.get(&b.corpus)))
            .then(a.path.cmp(&b.path))
            .then(a.start.cmp(&b.start))
    });
    Ok(list
        .into_iter()
        .map(|c| {
            json!({
                "repo": names.get(&c.corpus).cloned().unwrap_or_else(|| c.corpus.to_string()),
                "path": c.path,
                "qualified": c.qualified,
                "lines": [c.start, c.end],
                "relation": c.relation,
                "score": (c.score * 1000.0).round() / 1000.0,
            })
        })
        .collect())
}

pub fn unit_exists(conn: &Connection, corpus: &Target, path: &str, qualified: &str) -> Result<bool, String> {
    let found: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM file f JOIN unit u ON u.object_format = ?1 AND u.oid = f.oid \
             WHERE f.corpus_id = ?2 AND f.path = ?3 AND u.qualified = ?4",
            params![corpus.format, corpus.id, path, qualified],
            |r| r.get(0),
        )
        .map_err(sql)?;
    Ok(found > 0)
}

pub fn corpus_with_path(conn: &Connection, corpora: &[Target], path: &str) -> Result<Option<Target>, String> {
    for target in corpora {
        let found: i64 = conn
            .query_row("SELECT COUNT(*) FROM file WHERE corpus_id = ?1 AND path = ?2", params![target.id, path], |r| r.get(0))
            .map_err(sql)?;
        if found > 0 {
            return Ok(Some(target.clone()));
        }
    }
    Ok(None)
}

pub struct Excerpter {
    readers: HashMap<i64, Option<BlobReader>>,
}

impl Excerpter {
    pub fn new() -> Excerpter {
        Excerpter { readers: HashMap::new() }
    }

    pub fn excerpt(&mut self, conn: &Connection, corpora: &[Target], hit: &Hit, max_lines: usize) -> Option<Excerpt> {
        let target = corpora.iter().find(|t| t.name == hit.corpus)?;
        let (oid, start_byte, end_byte, start_line, licence): (String, i64, i64, i64, String) = conn
            .query_row(
                "SELECT s.oid, s.start_byte, s.end_byte, s.start_line, f.licence FROM search_unit s \
                 JOIN file f ON f.corpus_id = s.corpus_id AND f.path = s.path \
                 WHERE s.corpus_id = ?1 AND s.path = ?2 AND s.qualified = ?3 ORDER BY s.idx LIMIT 1",
                params![target.id, hit.path, hit.qualified],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .ok()?;
        let reader = self
            .readers
            .entry(target.id)
            .or_insert_with(|| Repo::open(Path::new(&target.path)).ok().and_then(|r| r.blob_reader().ok()))
            .as_mut()?;
        let bytes = reader.read(&oid).ok().flatten()?;
        let start = (start_byte.max(0) as usize).min(bytes.len());
        let end = (end_byte.max(0) as usize).min(bytes.len()).max(start);
        let full = String::from_utf8_lossy(&bytes[start..end]).to_string();
        let lines: Vec<&str> = full.lines().collect();
        let keep = lines.len().min(max_lines);
        let text = redact::text(&lines[..keep].join("\n")).0;
        Some(Excerpt {
            kind: "formslot.excerpt",
            source: Source {
                repo: target.name.clone(),
                path: hit.path.clone(),
                commit: target.head.clone(),
                lines: [start_line, start_line + keep as i64 - 1],
            },
            slot: None,
            copy: copy_verdict(&licence),
            licence,
            text,
        })
    }
}

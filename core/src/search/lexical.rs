use std::collections::HashSet;

use rusqlite::{Connection, params};

use super::text::{LEXICAL_TERM_CAP, fts_or, query_terms, query_terms_cap, split_words, stem, trigram_terms};
use super::{Hit, Target};
use crate::symbols::stable_id;

const RARE_IDF: f64 = 4.5;
const UNIT_COLUMNS: &str = "s.id, s.corpus_id, s.path, s.qualified, s.kind, s.signature, s.start_line, s.end_line";

fn sql(e: rusqlite::Error) -> String {
    format!("search: {e}")
}

pub struct Row {
    pub id: i64,
    pub corpus_id: i64,
    pub path: String,
    pub qualified: String,
    pub kind: String,
    pub signature: String,
    pub start_line: i64,
    pub end_line: i64,
}

fn row_of(r: &rusqlite::Row) -> rusqlite::Result<Row> {
    Ok(Row {
        id: r.get(0)?,
        corpus_id: r.get(1)?,
        path: r.get(2)?,
        qualified: r.get(3)?,
        kind: r.get(4)?,
        signature: r.get(5)?,
        start_line: r.get(6)?,
        end_line: r.get(7)?,
    })
}

pub fn unit_hit(row: &Row, corpus: &str, why: String, score: f64) -> Hit {
    Hit {
        kind: "unit".to_string(),
        id: stable_id(corpus, &row.path, &row.qualified, &row.signature),
        corpus: corpus.to_string(),
        path: row.path.clone(),
        lines: Some([row.start_line, row.end_line]),
        score,
        why: vec![why],
        key: format!("unit\0{corpus}\0{}\0{}", row.path, row.qualified),
        unit_kind: row.kind.clone(),
        qualified: row.qualified.clone(),
    }
}

fn names(corpora: &[Target]) -> impl Fn(i64) -> String + '_ {
    move |id| {
        corpora
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.name.clone())
            .unwrap_or_default()
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Pool {
    All,
    Code,
    Text,
}

fn pool_filter(pool: Pool) -> &'static str {
    match pool {
        Pool::All => "",
        Pool::Code => " AND s.kind NOT IN ('section', 'text')",
        Pool::Text => " AND s.kind IN ('section', 'text')",
    }
}

pub fn lexical(conn: &Connection, corpora: &[Target], scope: Option<i64>, query: &str, limit: usize) -> crate::Result<Vec<Hit>> {
    lexical_in(conn, corpora, scope, query, limit, Pool::All)
}

pub fn lexical_in(
    conn: &Connection,
    corpora: &[Target],
    scope: Option<i64>,
    query: &str,
    limit: usize,
    pool: Pool,
) -> crate::Result<Vec<Hit>> {
    let Some(m) = fts_or(&query_terms_cap(query, LEXICAL_TERM_CAP)) else {
        return Ok(Vec::new());
    };
    let sql_text = format!(
        "SELECT {UNIT_COLUMNS}, bm25(unit_fts, 8.0, 6.0, 2.0, 1.0) AS rank FROM unit_fts \
         JOIN search_unit s ON s.id = unit_fts.rowid \
         WHERE unit_fts MATCH ?1 AND (?2 IS NULL OR s.corpus_id = ?2){} \
         ORDER BY rank, s.path, s.qualified LIMIT ?3",
        pool_filter(pool)
    );
    collect(conn, &sql_text, params![m, scope, (limit * 3) as i64], corpora, limit, "lexical")
}

pub fn trigram(conn: &Connection, corpora: &[Target], scope: Option<i64>, query: &str, limit: usize) -> crate::Result<Vec<Hit>> {
    trigram_in(conn, corpora, scope, query, limit, Pool::All)
}

pub fn trigram_in(
    conn: &Connection,
    corpora: &[Target],
    scope: Option<i64>,
    query: &str,
    limit: usize,
    pool: Pool,
) -> crate::Result<Vec<Hit>> {
    let Some(m) = fts_or(&trigram_terms(query)) else {
        return Ok(Vec::new());
    };
    let sql_text = format!(
        "SELECT {UNIT_COLUMNS}, bm25(unit_tri, 5.0, 3.0, 2.0) AS rank FROM unit_tri \
         JOIN search_unit s ON s.id = unit_tri.rowid \
         WHERE unit_tri MATCH ?1 AND (?2 IS NULL OR s.corpus_id = ?2){} \
         ORDER BY rank, s.path, s.qualified LIMIT ?3",
        pool_filter(pool)
    );
    collect(conn, &sql_text, params![m, scope, (limit * 3) as i64], corpora, limit, "trigram")
}

fn collect(
    conn: &Connection,
    sql_text: &str,
    args: &[&dyn rusqlite::ToSql],
    corpora: &[Target],
    limit: usize,
    label: &str,
) -> crate::Result<Vec<Hit>> {
    let mut stmt = conn.prepare_cached(sql_text).map_err(sql)?;
    let rows = stmt
        .query_map(args, |r| Ok((row_of(r)?, r.get::<_, f64>(8)?)))
        .map_err(sql)?;
    let name_of = names(corpora);
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for row in rows {
        let (row, rank) = row.map_err(sql)?;
        let hit = unit_hit(&row, &name_of(row.corpus_id), String::new(), -rank);
        if !seen.insert(hit.key.clone()) {
            continue;
        }
        let mut hit = hit;
        hit.why = vec![format!("{label} #{}", out.len() + 1)];
        out.push(hit);
        if out.len() >= limit {
            break;
        }
    }
    Ok(out)
}

pub fn neighbours(conn: &Connection, corpora: &[Target], seeds: &[Hit], limit: usize) -> crate::Result<Vec<Hit>> {
    let mut weights: Vec<(String, i64, String, String, i64, Vec<String>)> = Vec::new();
    let seed_keys: HashSet<&str> = seeds.iter().map(|h| h.key.as_str()).collect();
    for seed in seeds {
        let Some(target) = corpora.iter().find(|c| c.name == seed.corpus) else {
            continue;
        };
        let qualified = seed.qualified.as_str();
        let mut stmt = conn
            .prepare_cached(
                "SELECT to_path, to_qualified, kind FROM edge WHERE corpus_id = ?1 AND from_path = ?2 AND from_qualified = ?3 \
                 UNION ALL \
                 SELECT from_path, from_qualified, kind FROM edge WHERE corpus_id = ?1 AND to_path = ?2 AND to_qualified = ?3",
            )
            .map_err(sql)?;
        let rows = stmt
            .query_map(params![target.id, seed.path, qualified], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
            })
            .map_err(sql)?;
        for row in rows {
            let (path, q, kind) = row.map_err(sql)?;
            if q.is_empty() {
                continue;
            }
            let key = format!("unit\0{}\0{}\0{}", target.name, path, q);
            if seed_keys.contains(key.as_str()) {
                continue;
            }
            let note = format!("{kind} link with {}", seed.path);
            match weights.iter_mut().find(|w| w.0 == target.name && w.2 == path && w.3 == q) {
                Some(w) => {
                    w.4 += 1;
                    if w.5.len() < 2 && !w.5.contains(&note) {
                        w.5.push(note);
                    }
                }
                None => weights.push((target.name.clone(), target.id, path, q, 1, vec![note])),
            }
        }
    }
    weights.sort_by(|a, b| b.4.cmp(&a.4).then_with(|| a.0.cmp(&b.0)).then_with(|| a.2.cmp(&b.2)).then_with(|| a.3.cmp(&b.3)));
    let mut out = Vec::new();
    for (corpus, corpus_id, path, q, count, notes) in weights {
        let found = conn
            .prepare_cached(&format!(
                "SELECT {UNIT_COLUMNS} FROM search_unit s WHERE s.corpus_id = ?1 AND s.path = ?2 AND s.qualified = ?3 ORDER BY s.idx LIMIT 1"
            ))
            .and_then(|mut s| s.query_row(params![corpus_id, path, q], row_of))
            .ok();
        let Some(row) = found else { continue };
        let mut hit = unit_hit(&row, &corpus, String::new(), count as f64);
        hit.why = vec![format!("graph #{} ({})", out.len() + 1, notes.join("; "))];
        out.push(hit);
        if out.len() >= limit {
            break;
        }
    }
    Ok(out)
}

pub fn memory(conn: &Connection, scope: Option<i64>, query: &str, limit: usize) -> crate::Result<Vec<Hit>> {
    memory_by(conn, scope, query, limit, None)
}

fn term_weights(conn: &Connection, query: &str) -> Vec<(String, f64)> {
    let total: f64 = conn
        .query_row("SELECT COUNT(*) FROM memory_record WHERE status = 'active'", [], |r| r.get::<_, i64>(0))
        .map(|n| n as f64)
        .unwrap_or(0.0);
    let mut out: Vec<(String, f64)> = Vec::new();
    for term in query_terms(query) {
        let Some(m) = fts_or(std::slice::from_ref(&term)) else { continue };
        let df: f64 = conn
            .prepare_cached(
                "SELECT COUNT(*) FROM memory_fts JOIN memory_record r ON r.id = memory_fts.rowid \
                 WHERE memory_fts MATCH ?1 AND r.status = 'active'",
            )
            .and_then(|mut s| s.query_row(params![m], |r| r.get::<_, i64>(0)))
            .map(|n| n as f64)
            .unwrap_or(0.0);
        if df == 0.0 {
            continue;
        }
        let idf = (1.0 + (total - df + 0.5) / (df + 0.5)).ln().max(0.0);
        let key = stem(&term);
        if !out.iter().any(|(k, _)| *k == key) {
            out.push((key, idf));
        }
    }
    out
}

type WeightKey = (String, String, i64, i64, i64, String);

thread_local! {
    static WEIGHTS: std::cell::RefCell<Option<(WeightKey, Vec<(String, f64)>)>> = const { std::cell::RefCell::new(None) };
}

fn weights_key(conn: &Connection, query: &str) -> Option<WeightKey> {
    let path = conn.path().map(str::to_string).filter(|p| !p.is_empty())?;
    let (all, active, top, stamp) = conn
        .prepare_cached(
            "SELECT COUNT(*), COALESCE(SUM(status = 'active'), 0), COALESCE(MAX(id), 0), COALESCE(MAX(updated_at), '') FROM memory_record",
        )
        .and_then(|mut s| s.query_row([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, String>(3)?))))
        .ok()?;
    Some((path, query.to_string(), all, active, top, stamp))
}

fn weights_once(conn: &Connection, query: &str) -> Vec<(String, f64)> {
    let Some(key) = weights_key(conn, query) else {
        return term_weights(conn, query);
    };
    let hit = WEIGHTS.with(|c| c.borrow().as_ref().filter(|e| e.0 == key).map(|e| e.1.clone()));
    if let Some(found) = hit {
        return found;
    }
    let fresh = term_weights(conn, query);
    WEIGHTS.with(|c| *c.borrow_mut() = Some((key, fresh.clone())));
    fresh
}

pub fn covered(conn: &Connection, hits: Vec<Hit>, query: &str, share: f64) -> Vec<Hit> {
    let weights = weights_once(conn, query);
    let whole: f64 = weights.iter().map(|(_, w)| w).sum();
    if weights.is_empty() || whole <= 0.0 {
        return Vec::new();
    }
    hits.into_iter()
        .filter(|h| {
            let Ok(id) = h.id.parse::<i64>() else { return false };
            let Ok(text) = conn
                .prepare_cached("SELECT title || ' ' || text FROM memory_record WHERE id = ?1")
                .and_then(|mut s| s.query_row(params![id], |r| r.get::<_, String>(0)))
            else {
                return false;
            };
            let words: HashSet<String> = split_words(&text).iter().map(|w| stem(w)).collect();
            let found: Vec<f64> = weights.iter().filter(|(t, _)| words.contains(t)).map(|(_, w)| *w).collect();
            let mass: f64 = found.iter().sum();
            found.len() as f64 / weights.len() as f64 >= share
                || mass / whole >= share
                || found.iter().any(|w| *w >= RARE_IDF)
        })
        .collect()
}

pub fn memory_by(conn: &Connection, scope: Option<i64>, query: &str, limit: usize, derived: Option<bool>) -> crate::Result<Vec<Hit>> {
    let filter = match derived {
        None => "",
        Some(true) => " AND r.kind IN ('hashout', 'tombstone')",
        Some(false) => " AND r.kind NOT IN ('hashout', 'tombstone') AND NOT (r.kind = 'how-to' AND r.authority = 'derived')",
    };
    memory_filtered(conn, scope, query, limit, filter)
}

pub fn memory_notes(conn: &Connection, scope: Option<i64>, query: &str, limit: usize) -> crate::Result<Vec<Hit>> {
    memory_filtered(conn, scope, query, limit, " AND r.kind = 'how-to' AND r.authority = 'derived'")
}

fn memory_filtered(conn: &Connection, scope: Option<i64>, query: &str, limit: usize, filter: &str) -> crate::Result<Vec<Hit>> {
    let Some(m) = fts_or(&query_terms(query)) else {
        return Ok(Vec::new());
    };
    let mut stmt = conn
        .prepare_cached(&format!(
            "SELECT r.id, r.kind, r.title, COALESCE(r.anchor, ''), bm25(memory_fts, 6.0, 2.0, 4.0) AS rank \
             FROM memory_fts JOIN memory_record r ON r.id = memory_fts.rowid \
             WHERE memory_fts MATCH ?1 AND r.status = 'active' AND (r.corpus_id IS NULL OR ?2 IS NULL OR r.corpus_id = ?2){filter} \
             ORDER BY rank, r.id LIMIT ?3"
        ))
        .map_err(sql)?;
    let rows = stmt
        .query_map(params![m, scope, limit as i64], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, f64>(4)?,
            ))
        })
        .map_err(sql)?;
    let mut out = Vec::new();
    for row in rows {
        let (id, kind, title, anchor, rank) = row.map_err(sql)?;
        out.push(Hit {
            kind: "memory".to_string(),
            id: id.to_string(),
            corpus: String::new(),
            path: anchor,
            lines: None,
            score: -rank,
            why: vec![format!("memory #{} ({kind}: {title})", out.len() + 1)],
            key: format!("memory\0{id}"),
            unit_kind: kind,
            qualified: String::new(),
        });
    }
    Ok(out)
}

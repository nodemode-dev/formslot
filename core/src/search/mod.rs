use std::collections::HashMap;

use rusqlite::Connection;
use serde::Serialize;

pub mod docs;
pub mod index;
pub mod lexical;
pub mod map;
pub mod pack;
pub mod rank;
pub mod slots;
pub mod text;
pub mod tokens;

use crate::store::Store;

pub const RRF_K: f64 = 60.0;
const POOL_LEXICAL: usize = 100;
const POOL_TRIGRAM: usize = 50;
const POOL_GRAPH: usize = 30;
const POOL_MEMORY: usize = 20;
const POOL_RULES: usize = 60;
const POOL_TEXT_LEXICAL: usize = 40;
const POOL_TEXT_TRIGRAM: usize = 20;
const WEIGHT_TEXT: f64 = 1.0;
const GRAPH_SEEDS: usize = 8;
const SLOT_POOL: usize = 10;
const SLOT_FUSED: usize = 5;
const TEST_PENALTY: f64 = 0.5;
const TEXT_PENALTY: f64 = 0.8;
const WEIGHT_SLOT: f64 = 2.0;
const WEIGHT_FOREIGN_SLOT: f64 = 0.5;
const WEIGHT_MEMORY: f64 = 2.0;
const MEMORY_FUSED: usize = 4;
const MEMORY_COVERAGE: f64 = 0.3;
const WEIGHT_DERIVED: f64 = 0.7;
const WEIGHT_NOTES: f64 = 1.0;
const NOTES_COVERAGE: f64 = 0.3;
const NOTES_FUSED: usize = 3;

#[derive(Debug, Clone)]
pub struct Target {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub format: String,
    pub head: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Hit {
    pub kind: String,
    pub id: String,
    pub corpus: String,
    pub path: String,
    pub lines: Option<[i64; 2]>,
    pub score: f64,
    pub why: Vec<String>,
    #[serde(skip)]
    pub key: String,
    #[serde(skip)]
    pub unit_kind: String,
    #[serde(skip)]
    pub qualified: String,
}

pub struct Retrieval {
    pub hits: Vec<Hit>,
    pub slots: Vec<slots::Match>,
}

fn sql(e: rusqlite::Error) -> String {
    format!("search: {e}")
}

pub fn targets(conn: &Connection) -> crate::Result<Vec<Target>> {
    let mut stmt = conn
        .prepare("SELECT id, name, path, object_format, COALESCE(head, '') FROM corpus ORDER BY id")
        .map_err(sql)?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Target {
                id: r.get(0)?,
                name: r.get(1)?,
                path: r.get(2)?,
                format: r.get(3)?,
                head: r.get(4)?,
            })
        })
        .map_err(sql)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(sql)
}

pub fn resolve(conn: &Connection, key: Option<&str>) -> crate::Result<Vec<Target>> {
    let all = targets(conn)?;
    match key {
        Some(k) => {
            let found: Vec<Target> = all
                .into_iter()
                .filter(|t| t.name == k || t.id.to_string() == k)
                .collect();
            if found.is_empty() {
                return Err(format!("no corpus named {k}"));
            }
            Ok(found)
        }
        None => Ok(all),
    }
}

pub fn resolve_one(conn: &Connection, key: Option<&str>) -> crate::Result<Target> {
    let mut found = resolve(conn, key)?;
    match found.len() {
        0 => Err("no corpus is indexed yet".to_string()),
        1 => Ok(found.remove(0)),
        _ => Err("more than one corpus is indexed: name one with --corpus".to_string()),
    }
}

pub fn ensure(conn: &Connection, corpora: &[Target]) -> crate::Result<()> {
    for t in corpora {
        index::ensure(conn, t.id, &t.format, &t.path)?;
    }
    Ok(())
}

pub fn fuse(lists: Vec<Vec<Hit>>) -> Vec<Hit> {
    fuse_weighted(lists.into_iter().map(|l| (1.0, l)).collect())
}

pub fn is_test_path(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    path.starts_with("e2e/")
        || path.contains("/e2e/")
        || path.starts_with("tests/")
        || path.contains("/tests/")
        || path.contains("/test/")
        || name.starts_with("test_")
        || name.contains(".test.")
        || name.contains(".spec.")
        || name.contains("_test.")
}

pub fn is_text_unit(kind: &str) -> bool {
    kind == "section" || kind == "text"
}

pub fn fuse_weighted(lists: Vec<(f64, Vec<Hit>)>) -> Vec<Hit> {
    let mut order: Vec<String> = Vec::new();
    let mut merged: HashMap<String, Hit> = HashMap::new();
    for (weight, list) in lists {
        for (rank, hit) in list.into_iter().enumerate() {
            let add = weight / (RRF_K + (rank + 1) as f64);
            match merged.get_mut(&hit.key) {
                Some(existing) => {
                    existing.score += add;
                    existing.why.extend(hit.why);
                }
                None => {
                    order.push(hit.key.clone());
                    let mut h = hit;
                    h.score = add;
                    merged.insert(h.key.clone(), h);
                }
            }
        }
    }
    let mut out: Vec<Hit> = order.into_iter().filter_map(|k| merged.remove(&k)).collect();
    for h in out.iter_mut() {
        if h.kind == "unit" && is_test_path(&h.path) {
            h.score *= TEST_PENALTY;
        }
        if h.kind == "unit" && is_text_unit(&h.unit_kind) {
            h.score *= TEXT_PENALTY;
        }
    }
    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.key.cmp(&b.key))
    });
    out
}

pub fn retrieve(conn: &Connection, corpora: &[Target], scope: Option<i64>, query: &str) -> crate::Result<Retrieval> {
    retrieve_with(conn, corpora, scope, query, Vec::new())
}

pub fn retrieve_with(
    conn: &Connection,
    corpora: &[Target],
    scope: Option<i64>,
    query: &str,
    extra: Vec<(f64, Vec<Hit>)>,
) -> crate::Result<Retrieval> {
    use lexical::Pool;
    let lexical = lexical::lexical_in(conn, corpora, scope, query, POOL_LEXICAL, Pool::Code)?;
    let trigram = lexical::trigram_in(conn, corpora, scope, query, POOL_TRIGRAM, Pool::Code)?;
    let text_lexical = lexical::lexical_in(conn, corpora, scope, query, POOL_TEXT_LEXICAL, Pool::Text)?;
    let text_trigram = lexical::trigram_in(conn, corpora, scope, query, POOL_TEXT_TRIGRAM, Pool::Text)?;
    let seeds: Vec<Hit> = fuse(vec![lexical.clone(), trigram.clone()]).into_iter().take(GRAPH_SEEDS).collect();
    let graph = lexical::neighbours(conn, corpora, &seeds, POOL_GRAPH)?;
    let memory: Vec<Hit> = lexical::covered(
        conn,
        lexical::memory_by(conn, scope, query, POOL_RULES, Some(false))?,
        query,
        MEMORY_COVERAGE,
    )
    .into_iter()
    .take(MEMORY_FUSED)
    .collect();
    let derived = lexical::memory_by(conn, scope, query, POOL_MEMORY, Some(true))?;
    let notes: Vec<Hit> = lexical::covered(
        conn,
        lexical::memory_notes(conn, scope, query, POOL_MEMORY)?,
        query,
        NOTES_COVERAGE,
    )
    .into_iter()
    .take(NOTES_FUSED)
    .collect();
    let wt = WEIGHT_TEXT;
    let pool: Vec<Hit> = fuse_weighted(vec![
        (1.0, lexical.clone()),
        (1.0, trigram.clone()),
        (wt, text_lexical.clone()),
        (wt, text_trigram.clone()),
    ])
    .into_iter()
    .take(SLOT_POOL)
    .collect();
    let slot_matches = slots::rank(conn, query, &pool, corpora)?;
    let mut slot_hits: Vec<Hit> = Vec::new();
    let mut slot_weak: Vec<Hit> = Vec::new();
    for (h, m) in slots::hits(&slot_matches).into_iter().zip(slot_matches.iter()) {
        if m.grounded {
            slot_hits.push(h);
        } else {
            slot_weak.push(h);
        }
    }
    slot_hits.truncate(SLOT_FUSED);
    slot_weak.truncate(SLOT_FUSED);
    let mut lists = vec![
        (1.0, lexical),
        (1.0, trigram),
        (wt, text_lexical),
        (wt, text_trigram),
        (1.0, graph),
        (WEIGHT_FOREIGN_SLOT, slot_weak),
        (WEIGHT_MEMORY, memory),
        (WEIGHT_DERIVED, derived),
        (WEIGHT_NOTES, notes),
        (WEIGHT_SLOT, slot_hits),
    ];
    lists.extend(extra);
    let hits = fuse_weighted(lists);
    Ok(Retrieval {
        hits,
        slots: slot_matches,
    })
}

pub fn run(store: &mut Store, query: &str, corpus: Option<&str>, limit: usize) -> crate::Result<Vec<Hit>> {
    let corpora = resolve(store.conn(), corpus)?;
    ensure(store.conn(), &corpora)?;
    let scope = corpus.and_then(|_| corpora.first().map(|c| c.id));
    let found = retrieve(store.conn(), &corpora, scope, query)?;
    let mut hits = found.hits;
    if limit > 0 {
        hits.truncate(limit);
    }
    Ok(hits)
}

pub fn rebuild(store: &mut Store, corpus: Option<&str>) -> crate::Result<Vec<(String, index::SyncStats)>> {
    let corpora = resolve(store.conn(), corpus)?;
    let mut out = Vec::new();
    for t in &corpora {
        let stats = index::rebuild(store.conn(), t.id, &t.format, &t.path)?;
        out.push((t.name.clone(), stats));
    }
    index::rebuild_memory(store.conn())?;
    Ok(out)
}

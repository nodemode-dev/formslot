use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use rusqlite::Connection;
use serde_json::Value;

use super::text::{query_terms, split_words, stem};
use super::{Hit, RRF_K, Target};
use crate::slots::storage;

const K1: f64 = 1.2;
const B: f64 = 0.75;
const FOREIGN_SLOT_COVERAGE: f64 = 0.6;
const WEIGHT_STRONG: f64 = 2.0;
const WEIGHT_NEUTRAL: f64 = 1.0;
const WEIGHT_WEAK: f64 = 0.5;
const STYLE_KINDS: [&str; 4] = ["selector", "element_id", "style", "custom_property"];

#[derive(Debug, Clone)]
pub struct Match {
    pub id: String,
    pub status: String,
    pub title: String,
    pub summary: String,
    pub origin_repo: String,
    pub files: Vec<(String, i64)>,
    pub score: f64,
    pub text_score: f64,
    pub grounded: bool,
    pub weight: f64,
    pub why: String,
}

struct Doc {
    m: Match,
    tf: HashMap<String, f64>,
    len: f64,
    rule: bool,
}

const RULE_DOCS: [&str; 3] = ["CLAUDE.md", "AGENTS.md", "NOTICE.md"];
const DEEP_PATH_RANK: usize = 40;

fn is_rule_slot(id: &str, kind: &str, files: &[(String, i64)]) -> bool {
    kind == "rule"
        || id.starts_with("rule.")
        || (!files.is_empty() && files.iter().all(|(p, _)| RULE_DOCS.contains(&p.as_str())))
}

fn str_of<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn add(tf: &mut HashMap<String, f64>, len: &mut f64, text: &str, weight: f64) {
    for w in split_words(text) {
        *len += weight;
        *tf.entry(stem(&w)).or_insert(0.0) += weight;
    }
}

type DocsKey = (String, i64, String, u64);

thread_local! {
    static DOCS: std::cell::RefCell<Option<(DocsKey, Rc<Vec<Doc>>)>> = const { std::cell::RefCell::new(None) };
}

fn docs_key(conn: &Connection) -> Option<DocsKey> {
    let path = conn.path().map(str::to_string).filter(|p| !p.is_empty())?;
    let mut stmt = conn
        .prepare_cached("SELECT id, status, version, content_hash, updated_at FROM slot ORDER BY id")
        .ok()?;
    let mut rows = stmt.query([]).ok()?;
    let mut hasher = DefaultHasher::new();
    let mut count = 0i64;
    let mut stamp = String::new();
    while let Some(r) = rows.next().ok()? {
        for i in 0..5 {
            r.get::<_, String>(i).ok()?.hash(&mut hasher);
        }
        let at: String = r.get(4).ok()?;
        if at > stamp {
            stamp = at;
        }
        count += 1;
    }
    Some((path, count, stamp, hasher.finish()))
}

fn docs(conn: &Connection) -> crate::Result<Rc<Vec<Doc>>> {
    let Some(key) = docs_key(conn) else {
        return Ok(Rc::new(build_docs(conn)?));
    };
    let hit = DOCS.with(|c| c.borrow().as_ref().filter(|e| e.0 == key).map(|e| e.1.clone()));
    if let Some(found) = hit {
        return Ok(found);
    }
    let fresh = Rc::new(build_docs(conn)?);
    DOCS.with(|c| *c.borrow_mut() = Some((key, fresh.clone())));
    Ok(fresh)
}

fn build_docs(conn: &Connection) -> crate::Result<Vec<Doc>> {
    let mut out = Vec::new();
    for row in storage::list(conn, None)? {
        if row.status != "canonical" && row.status != "proposed" {
            continue;
        }
        let m = &row.manifest;
        let mut tf = HashMap::new();
        let mut len = 0.0;
        add(&mut tf, &mut len, &row.id, 3.0);
        add(&mut tf, &mut len, str_of(m, "title"), 3.0);
        if let Some(tags) = m.get("tags").and_then(Value::as_array) {
            for t in tags.iter().filter_map(Value::as_str) {
                add(&mut tf, &mut len, t, 3.0);
            }
        }
        let mut files = Vec::new();
        if let Some(list) = m.get("files").and_then(Value::as_array) {
            for f in list {
                let path = str_of(f, "path").to_string();
                add(&mut tf, &mut len, &path, 2.0);
                files.push((path, f.get("lines").and_then(Value::as_i64).unwrap_or(0)));
            }
        }
        if let Some(list) = m.pointer("/api/exports").and_then(Value::as_array) {
            for e in list {
                add(&mut tf, &mut len, str_of(e, "symbol"), 2.0);
            }
        }
        add(&mut tf, &mut len, str_of(m, "summary"), 1.0);
        add(&mut tf, &mut len, str_of(m, "description"), 1.0);
        let rule = is_rule_slot(&row.id, str_of(m, "kind"), &files);
        out.push(Doc {
            rule,
            m: Match {
                id: row.id.clone(),
                status: row.status.clone(),
                title: str_of(m, "title").to_string(),
                summary: str_of(m, "summary").to_string(),
                origin_repo: m.pointer("/origin/repo").and_then(Value::as_str).unwrap_or("").to_string(),
                files,
                score: 0.0,
                text_score: 0.0,
                grounded: false,
                weight: 0.0,
                why: String::new(),
            },
            tf,
            len,
        });
    }
    Ok(out)
}

pub fn all(conn: &Connection) -> crate::Result<Vec<Match>> {
    Ok(docs(conn)?.iter().map(|d| d.m.clone()).collect())
}

pub fn rank(conn: &Connection, query: &str, units: &[Hit], deep: &[Hit], corpora: &[Target]) -> crate::Result<Vec<Match>> {
    let docs = docs(conn)?;
    if docs.is_empty() {
        return Ok(Vec::new());
    }
    let terms: Vec<String> = query_terms(query).iter().map(|t| stem(t)).collect();
    let n = docs.len() as f64;
    let avg = docs.iter().map(|d| d.len).sum::<f64>() / n;
    let mut text: Vec<(usize, f64)> = Vec::new();
    let mut coverage = vec![0.0f64; docs.len()];
    let mut whole = 0.0f64;
    for t in &terms {
        let df = docs.iter().filter(|o| o.tf.contains_key(t)).count() as f64;
        if df > 0.0 {
            whole += (1.0 + (n - df + 0.5) / (df + 0.5)).ln();
        }
    }
    for (i, d) in docs.iter().enumerate() {
        let mut score = 0.0;
        for t in &terms {
            let df = docs.iter().filter(|o| o.tf.contains_key(t)).count() as f64;
            let tf = d.tf.get(t).copied().unwrap_or(0.0);
            if tf == 0.0 {
                continue;
            }
            let idf = (1.0 + (n - df + 0.5) / (df + 0.5)).ln();
            coverage[i] += idf / whole;
            score += idf * tf * (K1 + 1.0) / (tf + K1 * (1.0 - B + B * d.len / avg));
        }
        if score > 0.0 {
            text.push((i, score));
        }
    }
    text.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then_with(|| docs[a.0].m.id.cmp(&docs[b.0].m.id)));
    let mut files: Vec<(usize, f64)> = Vec::new();
    for (i, d) in docs.iter().enumerate() {
        let mut score = 0.0;
        let source = if d.rule { deep } else { units };
        for (rank, h) in source.iter().filter(|h| !STYLE_KINDS.contains(&h.unit_kind.as_str())).enumerate() {
            let same_repo = d.rule
                || d.m.origin_repo.is_empty()
                || h.corpus == d.m.origin_repo
                || !corpora.iter().any(|c| c.name == d.m.origin_repo);
            if same_repo && d.m.files.iter().any(|(p, _)| *p == h.path) {
                score += 1.0 / (RRF_K + (rank + 1) as f64);
            }
        }
        if score > 0.0 {
            files.push((i, score));
        }
    }
    files.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then_with(|| docs[a.0].m.id.cmp(&docs[b.0].m.id)));
    let text_scores: HashMap<usize, f64> = text.iter().cloned().collect();
    let need = FOREIGN_SLOT_COVERAGE;
    let in_files = |i: usize, list: &[Hit]| -> bool {
        list.iter()
            .filter(|h| !STYLE_KINDS.contains(&h.unit_kind.as_str()))
            .any(|h| docs[i].m.files.iter().any(|(p, _)| *p == h.path))
    };
    let weights: Vec<f64> = (0..docs.len())
        .map(|i| {
            let origin = &docs[i].m.origin_repo;
            let native = origin.is_empty() || corpora.iter().any(|c| c.name == *origin);
            if docs[i].rule {
                return if coverage[i] >= need || in_files(i, units) { WEIGHT_STRONG } else { WEIGHT_NEUTRAL };
            }
            if native || files.iter().any(|(j, _)| *j == i) {
                return WEIGHT_STRONG;
            }
            WEIGHT_WEAK + (WEIGHT_STRONG - WEIGHT_WEAK) * (coverage[i] / need).min(1.0)
        })
        .collect();
    let mut fused: HashMap<usize, (f64, Vec<String>)> = HashMap::new();
    for (r, (i, _)) in text.iter().enumerate() {
        let e = fused.entry(*i).or_insert((0.0, Vec::new()));
        e.0 += 1.0 / (RRF_K + (r + 1) as f64);
        e.1.push(format!("text #{}", r + 1));
    }
    for (r, (i, _)) in files.iter().enumerate() {
        let e = fused.entry(*i).or_insert((0.0, Vec::new()));
        e.0 += 1.0 / (RRF_K + (r + 1) as f64);
        e.1.push(format!("files #{}", r + 1));
    }
    let mut out: Vec<(usize, Match)> = fused
        .into_iter()
        .map(|(i, (score, why))| {
            let mut m = docs[i].m.clone();
            m.score = score;
            m.text_score = text_scores.get(&i).copied().unwrap_or(0.0);
            m.weight = weights[i];
            m.why = why.join(", ");
            (i, m)
        })
        .collect();
    out.sort_by(|(_, a), (_, b)| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.text_score.partial_cmp(&a.text_score).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| (b.status == "canonical").cmp(&(a.status == "canonical")))
            .then_with(|| a.id.cmp(&b.id))
    });
    if let Some((i, m)) = out.first_mut()
        && m.weight < WEIGHT_NEUTRAL
        && in_files(*i, &deep[..deep.len().min(DEEP_PATH_RANK)])
    {
        m.weight = WEIGHT_NEUTRAL;
    }
    for (_, m) in out.iter_mut() {
        m.grounded = m.weight >= WEIGHT_STRONG;
    }
    Ok(out.into_iter().map(|(_, m)| m).collect())
}

pub fn hits(matches: &[Match]) -> Vec<Hit> {
    matches
        .iter()
        .enumerate()
        .map(|(i, m)| Hit {
            kind: "slot".to_string(),
            id: m.id.clone(),
            corpus: m.origin_repo.clone(),
            path: m.files.first().map(|f| f.0.clone()).unwrap_or_default(),
            lines: None,
            score: m.score,
            why: vec![format!("slot #{} ({}, {})", i + 1, m.status, m.why)],
            key: format!("slot\0{}", m.id),
            unit_kind: String::new(),
            qualified: String::new(),
        })
        .collect()
}

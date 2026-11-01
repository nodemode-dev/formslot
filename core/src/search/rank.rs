use std::collections::BTreeMap;

use rusqlite::{Connection, params};

pub const DAMPING: f64 = 0.85;
pub const ITERATIONS: usize = 40;

fn sql(e: rusqlite::Error) -> String {
    format!("search: {e}")
}

pub fn pagerank(n: usize, edges: &[(usize, usize, f64)]) -> Vec<f64> {
    if n == 0 {
        return Vec::new();
    }
    let mut out_weight = vec![0.0f64; n];
    for &(from, _, w) in edges {
        out_weight[from] += w;
    }
    let base = 1.0 / n as f64;
    let mut rank = vec![base; n];
    for _ in 0..ITERATIONS {
        let dangling: f64 = (0..n).filter(|&i| out_weight[i] == 0.0).map(|i| rank[i]).sum();
        let mut next = vec![(1.0 - DAMPING) * base + DAMPING * dangling * base; n];
        for &(from, to, w) in edges {
            next[to] += DAMPING * rank[from] * w / out_weight[from];
        }
        rank = next;
    }
    rank
}

fn kind_weight(kind: &str) -> f64 {
    match kind {
        "call" | "type" | "invoke" => 1.0,
        "import" => 0.5,
        "css_var" => 0.4,
        _ => 0.5,
    }
}

#[derive(Debug, Clone)]
pub struct Node {
    pub path: String,
    pub qualified: String,
    pub kind: String,
    pub name: String,
    pub signature: String,
    pub exported: bool,
    pub line: i64,
}

pub struct Graph {
    pub nodes: Vec<Node>,
    pub rank: Vec<f64>,
    pub edges: usize,
}

pub fn build(conn: &Connection, corpus_id: i64) -> crate::Result<Graph> {
    let mut index: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut nodes: Vec<Node> = Vec::new();
    let mut stmt = conn
        .prepare(
            "SELECT path, qualified, kind, name, signature, exported, start_line FROM search_unit \
             WHERE corpus_id = ?1 ORDER BY path, qualified, idx",
        )
        .map_err(sql)?;
    let rows = stmt
        .query_map(params![corpus_id], |r| {
            Ok(Node {
                path: r.get(0)?,
                qualified: r.get(1)?,
                kind: r.get(2)?,
                name: r.get(3)?,
                signature: r.get(4)?,
                exported: r.get::<_, i64>(5)? != 0,
                line: r.get(6)?,
            })
        })
        .map_err(sql)?;
    for row in rows {
        let node = row.map_err(sql)?;
        let key = (node.path.clone(), node.qualified.clone());
        if index.contains_key(&key) {
            continue;
        }
        index.insert(key, nodes.len());
        nodes.push(node);
    }
    let mut files: Vec<String> = nodes.iter().map(|n| n.path.clone()).collect();
    files.dedup();
    for path in files {
        let key = (path.clone(), String::new());
        if !index.contains_key(&key) {
            index.insert(key, nodes.len());
            nodes.push(Node {
                path,
                qualified: String::new(),
                kind: "file".to_string(),
                name: String::new(),
                signature: String::new(),
                exported: false,
                line: 0,
            });
        }
    }
    let mut edge_stmt = conn
        .prepare(
            "SELECT from_path, from_qualified, kind, to_path, to_qualified, COUNT(*) FROM edge WHERE corpus_id = ?1 \
             GROUP BY from_path, from_qualified, kind, to_path, to_qualified \
             ORDER BY from_path, from_qualified, kind, to_path, to_qualified",
        )
        .map_err(sql)?;
    let rows = edge_stmt
        .query_map(params![corpus_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, i64>(5)?,
            ))
        })
        .map_err(sql)?;
    let find = |path: &str, q: &str| -> Option<usize> {
        index
            .get(&(path.to_string(), q.to_string()))
            .or_else(|| index.get(&(path.to_string(), String::new())))
            .copied()
    };
    let mut merged: BTreeMap<(usize, usize), f64> = BTreeMap::new();
    let mut raw = 0usize;
    for row in rows {
        let (fp, fq, kind, tp, tq, count) = row.map_err(sql)?;
        let (Some(from), Some(to)) = (find(&fp, &fq), find(&tp, &tq)) else {
            continue;
        };
        if from == to {
            continue;
        }
        raw += 1;
        *merged.entry((from, to)).or_insert(0.0) += kind_weight(&kind) * (count as f64).sqrt();
    }
    let edges: Vec<(usize, usize, f64)> = merged.into_iter().map(|((f, t), w)| (f, t, w)).collect();
    let rank = pagerank(nodes.len(), &edges);
    Ok(Graph {
        nodes,
        rank,
        edges: raw,
    })
}

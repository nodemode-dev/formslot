use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, params};
use serde_json::{Value, json};

fn sql(e: rusqlite::Error) -> String {
    format!("slots: {e}")
}

#[derive(Debug, Clone)]
struct Copy {
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

fn names(conn: &Connection) -> crate::Result<HashMap<i64, String>> {
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

fn push(
    found: &mut HashMap<(i64, String, String, i64, i64), Copy>,
    own: &HashSet<String>,
    origin: i64,
    copy: Copy,
) {
    if copy.corpus == origin && own.contains(&copy.path) {
        return;
    }
    let key = (copy.corpus, copy.path.clone(), copy.qualified.clone(), copy.start, copy.end);
    match found.get(&key) {
        Some(old)
            if rank(&old.relation) < rank(&copy.relation)
                || (rank(&old.relation) == rank(&copy.relation) && old.score >= copy.score) => {}
        _ => {
            found.insert(key, copy);
        }
    }
}

pub fn derive(conn: &Connection, origin: i64, paths: &[String]) -> crate::Result<Vec<Value>> {
    let own: HashSet<String> = paths.iter().cloned().collect();
    let mut found: HashMap<(i64, String, String, i64, i64), Copy> = HashMap::new();

    let mut groups = conn
        .prepare(
            "SELECT g.tier, m2.corpus_id, m2.path, m2.qualified, m2.start_line, m2.end_line \
             FROM clone_member m1 \
             JOIN clone_group g ON g.id = m1.group_id \
             JOIN clone_member m2 ON m2.group_id = m1.group_id \
             WHERE m1.corpus_id = ?1 AND m1.path = ?2 \
             AND NOT (m2.corpus_id = m1.corpus_id AND m2.path = m1.path \
                      AND m2.qualified = m1.qualified AND m2.start_line = m1.start_line)",
        )
        .map_err(sql)?;
    let mut pairs_a = conn
        .prepare(
            "SELECT tier, score, b_corpus, b_path, b_qualified, b_start, b_end \
             FROM clone_pair WHERE a_corpus = ?1 AND a_path = ?2",
        )
        .map_err(sql)?;
    let mut pairs_b = conn
        .prepare(
            "SELECT tier, score, a_corpus, a_path, a_qualified, a_start, a_end \
             FROM clone_pair WHERE b_corpus = ?1 AND b_path = ?2",
        )
        .map_err(sql)?;
    let mut related_a = conn
        .prepare(
            "SELECT reason, score, b_corpus, b_path, b_qualified, b_start, b_end \
             FROM related_pair WHERE a_corpus = ?1 AND a_path = ?2",
        )
        .map_err(sql)?;
    let mut related_b = conn
        .prepare(
            "SELECT reason, score, a_corpus, a_path, a_qualified, a_start, a_end \
             FROM related_pair WHERE b_corpus = ?1 AND b_path = ?2",
        )
        .map_err(sql)?;

    for path in paths {
        let rows = groups
            .query_map(params![origin, path], |r| {
                Ok(Copy {
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
            push(&mut found, &own, origin, row.map_err(sql)?);
        }
        for (stmt, prefix) in [(&mut pairs_a, "clone-"), (&mut pairs_b, "clone-")] {
            let rows = stmt
                .query_map(params![origin, path], |r| {
                    Ok(Copy {
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
                push(&mut found, &own, origin, row.map_err(sql)?);
            }
        }
        for stmt in [&mut related_a, &mut related_b] {
            let rows = stmt
                .query_map(params![origin, path], |r| {
                    Ok(Copy {
                        relation: format!("related-{}", r.get::<_, String>(0)?),
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
                push(&mut found, &own, origin, row.map_err(sql)?);
            }
        }
    }

    let names = names(conn)?;
    let mut list: Vec<Copy> = found.into_values().collect();
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

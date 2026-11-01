use std::process::ExitCode;

use formslot_core::dedup::{self, DriftRow};
use formslot_core::related;
use formslot_core::store::Store;
use formslot_core::config;
use rusqlite::{Connection, params};

const DEFAULT_LIMIT: usize = 200;
const RELATED_DEFAULT_LIMIT: usize = 0;

fn open() -> Result<Store, String> {
    Store::open(&config::corpus_db())
}

fn resolve(conn: &Connection, key: &str) -> Result<(i64, String), String> {
    conn.query_row(
        "SELECT id, name FROM corpus WHERE name = ?1 OR CAST(id AS TEXT) = ?1 ORDER BY name = ?1 DESC LIMIT 1",
        params![key],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .map_err(|_| format!("no corpus named or numbered '{key}'"))
}

fn option(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn limit_of(args: &[String]) -> Result<usize, String> {
    match option(args, "--limit") {
        None => Ok(DEFAULT_LIMIT),
        Some(v) => v.parse::<usize>().map_err(|_| "--limit needs a number".to_string()),
    }
}

fn positionals(args: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut skip = false;
    for arg in args {
        if skip {
            skip = false;
            continue;
        }
        if arg == "--limit" {
            skip = true;
            continue;
        }
        if !arg.starts_with("--") {
            out.push(arg.clone());
        }
    }
    out
}

fn fail(message: &str) -> ExitCode {
    eprintln!("formslot: {message}");
    ExitCode::from(1)
}

fn tier_title(tier: &str) -> &'static str {
    match tier {
        "T1" => "T1 exact copies (same text)",
        "T2" => "T2 renamed copies (same tokens)",
        _ => "T3 near copies (similar tokens)",
    }
}

fn print_groups(conn: &Connection, scope: &str, limit: usize) -> Result<(), String> {
    let query = format!(
        "SELECT g.id, g.tier, g.members FROM clone_group g
         WHERE {scope}
         ORDER BY g.tier,
                  (SELECT MAX(end_line - start_line) FROM clone_member WHERE group_id = g.id) DESC,
                  g.members DESC, g.id
         LIMIT ?1"
    );
    let mut stmt = conn.prepare(&query).map_err(|e| e.to_string())?;
    let groups: Vec<(i64, String, i64)> = stmt
        .query_map(params![if limit == 0 { -1 } else { limit as i64 }], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    if groups.is_empty() {
        return Ok(());
    }
    println!("clone groups");
    let mut members = conn
        .prepare("SELECT corpus_id, path, qualified, start_line, end_line FROM clone_member WHERE group_id = ?1 ORDER BY corpus_id, path, start_line")
        .map_err(|e| e.to_string())?;
    for (id, tier, count) in groups {
        println!("  {tier} group of {count}");
        let rows = members
            .query_map(params![id], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?, r.get::<_, i64>(4)?))
            })
            .map_err(|e| e.to_string())?;
        for row in rows.filter_map(|r| r.ok()) {
            println!("    [{}] {}:{}-{} {}", row.0, row.1, row.3, row.4, row.2);
        }
    }
    println!();
    Ok(())
}

fn print_pairs(conn: &Connection, scope: &str, limit: usize) -> Result<usize, String> {
    let query = format!(
        "SELECT tier, score, a_path, a_qualified, a_start, a_end, b_path, b_qualified, b_start, b_end
         FROM clone_pair WHERE {scope}
         ORDER BY tier, (a_end - a_start) DESC, score DESC, a_path, a_start"
    );
    let mut stmt = conn.prepare(&query).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, f64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, i64>(8)?,
                r.get::<_, i64>(9)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut current = String::new();
    let mut printed = 0usize;
    let mut total = 0usize;
    for row in rows.filter_map(|r| r.ok()) {
        total += 1;
        if limit != 0 && printed >= limit {
            continue;
        }
        if row.0 != current {
            if !current.is_empty() {
                println!();
            }
            println!("{}", tier_title(&row.0));
            current = row.0.clone();
        }
        println!(
            "{} score {:.2}  {}:{}-{} {}  <->  {}:{}-{} {}",
            row.0, row.1, row.2, row.4, row.5, row.3, row.6, row.8, row.9, row.7
        );
        printed += 1;
    }
    Ok(total)
}

struct RelatedRow {
    reason: String,
    score: f64,
    a: (i64, String, String, i64, i64),
    b: (i64, String, String, i64, i64),
}

fn family_of(qualified: &str) -> String {
    related::normalise_name(related::last_segment(qualified))
}

fn print_related(conn: &Connection, scope: &str, limit: usize) -> Result<usize, String> {
    let query = format!(
        "SELECT reason, score, a_corpus, a_path, a_qualified, a_start, a_end,
                b_corpus, b_path, b_qualified, b_start, b_end
         FROM related_pair WHERE {scope}"
    );
    let mut stmt = conn.prepare(&query).map_err(|e| e.to_string())?;
    let rows: Vec<RelatedRow> = stmt
        .query_map([], |r| {
            Ok(RelatedRow {
                reason: r.get(0)?,
                score: r.get(1)?,
                a: (r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?),
                b: (r.get(7)?, r.get(8)?, r.get(9)?, r.get(10)?, r.get(11)?),
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    let total = rows.len();
    let named: std::collections::HashSet<String> = rows
        .iter()
        .filter(|r| r.reason == "same_name")
        .map(|r| family_of(&r.a.2))
        .collect();
    let keys: Vec<String> = rows
        .iter()
        .map(|r| {
            let (a, b) = (family_of(&r.a.2), family_of(&r.b.2));
            if r.reason == "same_name" || named.contains(&a) || !named.contains(&b) {
                a
            } else {
                b
            }
        })
        .collect();
    let mut best: std::collections::HashMap<&str, f64> = std::collections::HashMap::new();
    for (i, row) in rows.iter().enumerate() {
        let slot = best.entry(keys[i].as_str()).or_insert(0.0);
        if row.score > *slot {
            *slot = row.score;
        }
    }
    let mut order: Vec<usize> = (0..rows.len()).collect();
    order.sort_by(|&x, &y| {
        let (rx, ry) = (&rows[x], &rows[y]);
        best[keys[y].as_str()]
            .partial_cmp(&best[keys[x].as_str()])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(keys[x].cmp(&keys[y]))
            .then((rx.reason != "same_name").cmp(&(ry.reason != "same_name")))
            .then(ry.score.partial_cmp(&rx.score).unwrap_or(std::cmp::Ordering::Equal))
            .then(rx.a.1.cmp(&ry.a.1))
            .then(rx.a.3.cmp(&ry.a.3))
    });
    let mut current: Option<&str> = None;
    let mut printed = 0usize;
    for i in order {
        if limit != 0 && printed >= limit {
            break;
        }
        if current.is_some() && current != Some(keys[i].as_str()) {
            println!();
        }
        current = Some(keys[i].as_str());
        let row = &rows[i];
        println!(
            "R {} {:.2}  {}:{}-{} {}  <->  {}:{}-{} {}",
            row.reason, row.score, row.a.1, row.a.3, row.a.4, row.a.2, row.b.1, row.b.3, row.b.4, row.b.2
        );
        printed += 1;
    }
    Ok(total)
}

fn show(conn: &Connection, related: bool, groups: &str, pairs: &str, limit: usize) -> Result<usize, String> {
    if related {
        print_related(conn, pairs, limit)
    } else {
        print_groups(conn, groups, limit).and_then(|_| print_pairs(conn, pairs, limit))
    }
}

pub fn dupes(args: &[String]) -> ExitCode {
    let limit = match limit_of(args) {
        Ok(v) => v,
        Err(e) => return fail(&e),
    };
    let across = args.iter().any(|a| a == "--across");
    let related = args.iter().any(|a| a == "--related");
    let limit = if related && option(args, "--limit").is_none() { RELATED_DEFAULT_LIMIT } else { limit };
    let names = positionals(args);
    let store = match open() {
        Ok(s) => s,
        Err(e) => return fail(&e),
    };
    let conn = store.conn();
    let result = if across {
        if names.len() != 2 {
            return fail("usage: formslot dupes --across <corpusA> <corpusB> [--limit N] [--related]");
        }
        let (a, a_name) = match resolve(conn, &names[0]) {
            Ok(v) => v,
            Err(e) => return fail(&e),
        };
        let (b, b_name) = match resolve(conn, &names[1]) {
            Ok(v) => v,
            Err(e) => return fail(&e),
        };
        println!("{} between {a_name} and {b_name}\n", if related { "related code" } else { "clones" });
        let groups = format!(
            "g.id IN (SELECT group_id FROM clone_member WHERE corpus_id = {a}) AND g.id IN (SELECT group_id FROM clone_member WHERE corpus_id = {b})"
        );
        let pairs = format!(
            "((a_corpus = {a} AND b_corpus = {b}) OR (a_corpus = {b} AND b_corpus = {a}))"
        );
        show(conn, related, &groups, &pairs, limit)
    } else {
        if names.len() != 1 {
            return fail("usage: formslot dupes <corpus> [--limit N] [--related]  or  formslot dupes --across <corpusA> <corpusB> [--related]");
        }
        let (id, name) = match resolve(conn, &names[0]) {
            Ok(v) => v,
            Err(e) => return fail(&e),
        };
        println!("{} inside {name}\n", if related { "related code" } else { "clones" });
        let groups = format!(
            "g.id IN (SELECT group_id FROM clone_member WHERE corpus_id = {id} GROUP BY group_id HAVING COUNT(*) >= 2)"
        );
        let pairs = format!("a_corpus = {id} AND b_corpus = {id}");
        show(conn, related, &groups, &pairs, limit)
    };
    match result {
        Ok(0) => {
            if related {
                println!("no related pairs found; run an index first so the units and prints exist");
            } else {
                println!("no clones found; run an index first so the units and prints exist");
            }
            ExitCode::SUCCESS
        }
        Ok(total) => {
            println!("\n{total} pairs");
            ExitCode::SUCCESS
        }
        Err(e) => fail(&e),
    }
}

fn print_drift(row: &DriftRow) {
    println!(
        "{} {:.2}  {}:{}-{} {}  <->  {}:{}-{} {}",
        row.state, row.score, row.a_path, row.a_start, row.a_end, row.a_qualified, row.b_path, row.b_start, row.b_end, row.b_qualified
    );
}

pub fn drift(args: &[String]) -> ExitCode {
    let limit = match limit_of(args) {
        Ok(v) => v,
        Err(e) => return fail(&e),
    };
    let all = args.iter().any(|a| a == "--all");
    let names = positionals(args);
    if names.len() != 2 {
        return fail("usage: formslot drift <corpusA> <corpusB> [--all] [--limit N]");
    }
    let store = match open() {
        Ok(s) => s,
        Err(e) => return fail(&e),
    };
    let conn = store.conn();
    let (a, a_name) = match resolve(conn, &names[0]) {
        Ok(v) => v,
        Err(e) => return fail(&e),
    };
    let (b, b_name) = match resolve(conn, &names[1]) {
        Ok(v) => v,
        Err(e) => return fail(&e),
    };
    let rows = match dedup::drift(conn, a, b) {
        Ok(v) => v,
        Err(e) => return fail(&e),
    };
    println!("drift from {a_name} to {b_name}");
    for state in ["identical", "renamed", "drifted", "diverged"] {
        let count = rows.iter().filter(|r| r.state == state).count();
        println!("  {state}: {count}");
    }
    println!();
    let mut printed = 0usize;
    for row in rows.iter().filter(|r| all || r.state != "identical") {
        if limit != 0 && printed >= limit {
            println!("(more rows hidden; raise --limit)");
            break;
        }
        print_drift(row);
        printed += 1;
    }
    ExitCode::SUCCESS
}

use std::process::ExitCode;

use formslot_core::store::Store;
use formslot_core::symbols::stable_id;
use formslot_core::config;

const LIMIT: usize = 60;

fn option(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn positionals(args: &[String]) -> Vec<String> {
    let mut found = Vec::new();
    let mut skip = false;
    for arg in args {
        if skip {
            skip = false;
            continue;
        }
        if matches!(arg.as_str(), "--lang" | "--kind") {
            skip = true;
            continue;
        }
        if !arg.starts_with("--") {
            found.push(arg.clone());
        }
    }
    found
}

fn fail(message: String) -> ExitCode {
    eprintln!("formslot: {message}");
    ExitCode::FAILURE
}

fn lit(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

fn like(text: &str) -> String {
    text.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

struct Target {
    id: i64,
    name: String,
    format: String,
}

fn resolve(store: &Store, key: &str) -> Result<Target, String> {
    let by_name = format!(
        "SELECT id, name, object_format FROM corpus WHERE name = {} OR CAST(id AS TEXT) = {} ORDER BY name = {} DESC LIMIT 1",
        lit(key),
        lit(key),
        lit(key)
    );
    store
        .conn()
        .query_row(&by_name, (), |r| Ok(Target { id: r.get(0)?, name: r.get(1)?, format: r.get(2)? }))
        .map_err(|_| format!("no corpus named {key}"))
}

fn rows(store: &Store, sql: &str, width: usize) -> Result<Vec<Vec<String>>, String> {
    let mut stmt = store.conn().prepare(sql).map_err(|e| e.to_string())?;
    let mapped = stmt
        .query_map((), |r| {
            let mut row = Vec::with_capacity(width);
            for i in 0..width {
                let value: String = r.get(i)?;
                row.push(value);
            }
            Ok(row)
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in mapped {
        out.push(row.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

fn open() -> Result<Store, String> {
    Store::open(&config::corpus_db())
}

const ROLES: &str = "f.role IN ('source', 'data', 'doc')";

pub fn symbols(args: &[String]) -> ExitCode {
    let given = positionals(args);
    let Some(key) = given.first() else {
        return fail("symbols needs a corpus name".to_string());
    };
    let store = match open() {
        Ok(s) => s,
        Err(e) => return fail(e),
    };
    let target = match resolve(&store, key) {
        Ok(t) => t,
        Err(e) => return fail(e),
    };
    let mut filter = format!("f.corpus_id = {} AND {ROLES}", target.id);
    if let Some(lang) = option(args, "--lang") {
        filter.push_str(&format!(" AND f.language = {}", lit(&lang)));
    }
    if let Some(kind) = option(args, "--kind") {
        filter.push_str(&format!(" AND u.kind = {}", lit(&kind)));
    }
    if flag(args, "--exported") {
        filter.push_str(" AND u.exported = 1");
    }
    let join = format!(
        "FROM file f JOIN unit u ON u.object_format = {} AND u.oid = f.oid WHERE {filter}",
        lit(&target.format)
    );
    if flag(args, "--count") {
        let sql = format!("SELECT CAST(COUNT(*) AS TEXT) {join}");
        return match rows(&store, &sql, 1) {
            Ok(found) => {
                println!("{}", found.first().and_then(|r| r.first()).cloned().unwrap_or_default());
                ExitCode::SUCCESS
            }
            Err(e) => fail(e),
        };
    }
    let sql = format!(
        "SELECT f.path, CAST(u.start_line AS TEXT), u.kind, u.qualified {join} ORDER BY f.path, u.start_line, u.idx"
    );
    match rows(&store, &sql, 4) {
        Ok(found) => {
            for row in found {
                println!("{}:{} {} {}", row[0], row[1], row[2], row[3]);
            }
            ExitCode::SUCCESS
        }
        Err(e) => fail(e),
    }
}

fn shown(found: &[Vec<String>], format: impl Fn(&Vec<String>) -> String) {
    for row in found.iter().take(LIMIT) {
        println!("  {}", format(row));
    }
    if found.len() > LIMIT {
        println!("  ... {} more", found.len() - LIMIT);
    }
}

pub fn show(args: &[String]) -> ExitCode {
    let given = positionals(args);
    let (Some(key), Some(spec)) = (given.first(), given.get(1)) else {
        return fail("show needs a corpus and <path>#<qualified>".to_string());
    };
    let Some((path, qualified)) = spec.split_once('#') else {
        return fail("show needs <path>#<qualified>".to_string());
    };
    let store = match open() {
        Ok(s) => s,
        Err(e) => return fail(e),
    };
    let target = match resolve(&store, key) {
        Ok(t) => t,
        Err(e) => return fail(e),
    };
    let sql = format!(
        "SELECT u.kind, CAST(u.start_line AS TEXT), CAST(u.end_line AS TEXT), u.signature, CAST(u.exported AS TEXT), u.sig8, f.oid \
         FROM file f JOIN unit u ON u.object_format = {} AND u.oid = f.oid \
         WHERE f.corpus_id = {} AND f.path = {} AND u.qualified = {} ORDER BY u.idx",
        lit(&target.format),
        target.id,
        lit(path),
        lit(qualified)
    );
    let units = match rows(&store, &sql, 7) {
        Ok(u) => u,
        Err(e) => return fail(e),
    };
    if units.is_empty() {
        return fail(format!("no unit {path}#{qualified} in {}", target.name));
    }
    for unit in &units {
        println!("{path}:{}-{} {} {qualified}", unit[1], unit[2], unit[0]);
        println!("  id: {}", stable_id(&target.name, path, qualified, &unit[3]));
        println!("  exported: {}", if unit[4] == "1" { "yes" } else { "no" });
        println!("  signature: {}", unit[3]);
        let made = format!(
            "SELECT r.kind, r.name, CAST(r.line AS TEXT) FROM unit_ref r \
             WHERE r.object_format = {} AND r.oid = {} AND r.line BETWEEN {} AND {} ORDER BY r.line",
            lit(&target.format),
            lit(&unit[6]),
            unit[1],
            unit[2]
        );
        match rows(&store, &made, 3) {
            Ok(found) => {
                println!("  makes {} references:", found.len());
                shown(&found, |r| format!("{}:{} {} {}", path, r[2], r[0], r[1]));
            }
            Err(e) => return fail(e),
        }
        let out = format!(
            "SELECT CAST(line AS TEXT), kind, to_path, to_qualified FROM edge \
             WHERE corpus_id = {} AND from_path = {} AND from_qualified = {} ORDER BY line",
            target.id,
            lit(path),
            lit(qualified)
        );
        match rows(&store, &out, 4) {
            Ok(found) => {
                println!("  resolves to {} edges:", found.len());
                shown(&found, |r| format!("{}:{} {} -> {}#{}", path, r[0], r[1], r[2], r[3]));
            }
            Err(e) => return fail(e),
        }
        let inbound = format!(
            "SELECT from_path, CAST(line AS TEXT), kind, from_qualified FROM edge \
             WHERE corpus_id = {} AND to_path = {} AND to_qualified = {} ORDER BY from_path, line",
            target.id,
            lit(path),
            lit(qualified)
        );
        match rows(&store, &inbound, 4) {
            Ok(found) => {
                let mut files: Vec<&String> = found.iter().map(|r| &r[0]).collect();
                files.sort();
                files.dedup();
                println!("  referenced by {} edges in {} files:", found.len(), files.len());
                shown(&found, |r| {
                    let from = if r[3].is_empty() { String::new() } else { format!(" [{}]", r[3]) };
                    format!("{}:{} {}{}", r[0], r[1], r[2], from)
                });
            }
            Err(e) => return fail(e),
        }
    }
    ExitCode::SUCCESS
}

pub fn refs(args: &[String]) -> ExitCode {
    let given: Vec<String> = args.iter().filter(|a| a.as_str() != "--").cloned().collect();
    let (Some(key), Some(name)) = (given.first(), given.get(1)) else {
        return fail("refs needs a corpus and a name".to_string());
    };
    let store = match open() {
        Ok(s) => s,
        Err(e) => return fail(e),
    };
    let target = match resolve(&store, key) {
        Ok(t) => t,
        Err(e) => return fail(e),
    };
    let pattern = like(name);
    let sql = format!(
        "SELECT from_path, CAST(line AS TEXT), kind, to_path, to_qualified FROM edge \
         WHERE corpus_id = {} AND (to_qualified = {} OR to_qualified LIKE {} ESCAPE '\\' OR to_qualified LIKE {} ESCAPE '\\') \
         ORDER BY from_path, line",
        target.id,
        lit(name),
        lit(&format!("%.{pattern}")),
        lit(&format!("%::{pattern}"))
    );
    match rows(&store, &sql, 5) {
        Ok(found) => {
            for row in found {
                println!("{}:{} {} -> {}#{}", row[0], row[1], row[2], row[3], row[4]);
            }
            ExitCode::SUCCESS
        }
        Err(e) => fail(e),
    }
}

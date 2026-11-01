use std::collections::BTreeMap;
use std::process::ExitCode;

use formslot_core::config;
use formslot_core::memory::{self, Filter, KINDS, STATUSES, propose, recall};
use formslot_core::store::Store;
use serde_json::{Value, json};

const VALUED: [&str; 8] = ["--kind", "--status", "--corpus", "--limit", "--anchor", "--propose", "--format", "--check"];

fn option(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn positionals(args: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut skip = false;
    for arg in args {
        if skip {
            skip = false;
            continue;
        }
        if VALUED.contains(&arg.as_str()) {
            skip = true;
            continue;
        }
        if !arg.starts_with("--") {
            out.push(arg.clone());
        }
    }
    out
}

fn missing_value(args: &[String]) -> Option<String> {
    VALUED.iter().find_map(|name| {
        let at = args.iter().position(|a| a == name)?;
        match args.get(at + 1) {
            Some(value) if !value.starts_with("--") => None,
            _ => Some(format!("{name} needs a value")),
        }
    })
}

fn fail(message: &str) -> ExitCode {
    eprintln!("formslot: {message}");
    ExitCode::from(1)
}

fn usage(line: &str) -> ExitCode {
    eprintln!("usage: formslot {line}");
    ExitCode::from(2)
}

fn open() -> Result<Store, String> {
    Store::open(&config::corpus_db())
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}

fn clip(text: &str, max: usize) -> String {
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        return flat;
    }
    let cut: String = flat.chars().take(max.saturating_sub(3)).collect();
    format!("{cut}...")
}

fn limit_of(args: &[String], default: usize) -> Result<usize, String> {
    match option(args, "--limit") {
        None => Ok(default),
        Some(raw) => raw.parse::<usize>().map_err(|_| format!("--limit needs a whole number, not {raw}")),
    }
}

fn kind_of(args: &[String]) -> Result<Option<String>, String> {
    match option(args, "--kind") {
        Some(kind) if !KINDS.contains(&kind.as_str()) => {
            Err(format!("--kind is one of {}", KINDS.join(", ")))
        }
        other => Ok(other),
    }
}

fn corpus_of(store: &Store, args: &[String]) -> Result<Option<i64>, String> {
    match option(args, "--corpus") {
        None => Ok(None),
        Some(key) => memory::corpus_of(store.conn(), &key).map(|(id, _)| Some(id)),
    }
}

fn print_counts(rows: &[(String, String, i64)]) {
    let mut table: BTreeMap<&str, BTreeMap<&str, i64>> = BTreeMap::new();
    for (kind, status, n) in rows {
        table.entry(kind.as_str()).or_default().insert(status.as_str(), *n);
    }
    println!("  {:<10} {:>7} {:>7} {:>11} {:>9} {:>7}", "kind", "active", "stale", "superseded", "proposed", "total");
    let mut totals = [0i64; 4];
    for kind in KINDS {
        let Some(by) = table.get(kind) else { continue };
        let cells: Vec<i64> = ["active", "stale", "superseded", "proposed"]
            .iter()
            .map(|s| by.get(s).copied().unwrap_or(0))
            .collect();
        for (i, c) in cells.iter().enumerate() {
            totals[i] += c;
        }
        println!(
            "  {:<10} {:>7} {:>7} {:>11} {:>9} {:>7}",
            kind,
            cells[0],
            cells[1],
            cells[2],
            cells[3],
            cells.iter().sum::<i64>()
        );
    }
    println!(
        "  {:<10} {:>7} {:>7} {:>11} {:>9} {:>7}",
        "all",
        totals[0],
        totals[1],
        totals[2],
        totals[3],
        totals.iter().sum::<i64>()
    );
}

pub fn run(args: &[String]) -> ExitCode {
    let sub = args.first().map(String::as_str).unwrap_or("");
    match sub {
        "ingest" => ingest(&args[1..]),
        "list" => list(&args[1..]),
        "declines" => declines(&args[1..]),
        _ => usage("memory ingest <corpus> [--json] | memory list [--kind K] [--status S] [--corpus C] [--limit N] [--count] [--json] | memory declines [--check <corpus>] [--json]"),
    }
}

fn ingest(args: &[String]) -> ExitCode {
    let pos = positionals(args);
    let Some(name) = pos.first() else {
        return usage("memory ingest <corpus> [--json]");
    };
    let mut store = match open() {
        Ok(store) => store,
        Err(e) => return fail(&e),
    };
    let report = match memory::ingest(&mut store, name) {
        Ok(report) => report,
        Err(e) => return fail(&e),
    };
    if flag(args, "--json") {
        println!("{}", pretty(&json!(report)));
        return ExitCode::SUCCESS;
    }
    println!("memory ingest {} at {}", report.corpus, report.commit.chars().take(12).collect::<String>());
    println!("  sources read   {}", report.sources);
    println!("  items parsed   {}", report.items);
    println!("  added          {}", report.added);
    println!("  unchanged      {}", report.unchanged);
    println!("  superseded     {}", report.superseded);
    println!("  stale          {}", report.stale);
    println!("  restored       {}", report.restored);
    println!(
        "  anchors        {} checked, {} newly anchored, {} marked stale, {} restored",
        report.anchors.checked, report.anchors.anchored, report.anchors.stale, report.anchors.restored
    );
    println!("records of {}:", report.corpus);
    print_counts(&report.counts);
    ExitCode::SUCCESS
}

fn declines(args: &[String]) -> ExitCode {
    if let Some(problem) = missing_value(args) {
        return fail(&problem);
    }
    let json_out = flag(args, "--json");
    let origin = memory::declines::source();
    let Some(corpus) = option(args, "--check") else {
        let specs = match memory::declines::specs() {
            Ok(specs) => specs,
            Err(e) => return fail(&e),
        };
        if json_out {
            println!("{}", pretty(&json!({"source": origin, "specs": specs})));
            return ExitCode::SUCCESS;
        }
        println!("declines from {origin}: {} specs", specs.len());
        for spec in &specs {
            let hint = if spec.corpus.is_empty() { "-" } else { spec.corpus.as_str() };
            println!("  {:<8} {}", hint, clip(&spec.title, 90));
        }
        return ExitCode::SUCCESS;
    };
    let store = match open() {
        Ok(store) => store,
        Err(e) => return fail(&e),
    };
    let (name, checks) = match memory::ingest::check_declines(&store, &corpus) {
        Ok(pair) => pair,
        Err(e) => return fail(&e),
    };
    let state = |c: &memory::declines::Check| -> &'static str {
        match (c.path.is_some(), c.expected) {
            (true, _) => "ok",
            (false, true) => "FAIL",
            (false, false) => "skip",
        }
    };
    let failed = checks.iter().filter(|c| state(c) == "FAIL").count();
    if json_out {
        let rows: Vec<Value> = checks
            .iter()
            .map(|c| json!({"state": state(c), "title": c.title, "corpus": c.corpus, "path": c.path, "line": c.line}))
            .collect();
        println!("{}", pretty(&json!({"corpus": name, "source": origin, "failed": failed, "specs": rows})));
    } else {
        println!("declines check {name} (specs from {origin})");
        for c in &checks {
            let place = match (&c.path, c.line) {
                (Some(path), Some(line)) => format!("{path}:{line}"),
                _ if c.expected => format!("no line in {name} holds the match phrases"),
                _ => format!("belongs to {}", c.corpus),
            };
            println!("  {:<5} {}  {}", state(c), clip(&c.title, 70), place);
        }
        let ok = checks.iter().filter(|c| state(c) == "ok").count();
        println!("{} specs: {ok} found, {} skipped, {failed} failed", checks.len(), checks.len() - ok - failed);
    }
    if failed > 0 {
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

fn list(args: &[String]) -> ExitCode {
    if let Some(problem) = missing_value(args) {
        return fail(&problem);
    }
    let kind = match kind_of(args) {
        Ok(kind) => kind,
        Err(e) => return fail(&e),
    };
    let status = option(args, "--status");
    if let Some(s) = &status
        && !STATUSES.contains(&s.as_str())
    {
        return fail(&format!("--status is one of {}", STATUSES.join(", ")));
    }
    let limit = match limit_of(args, 50) {
        Ok(n) => n,
        Err(e) => return fail(&e),
    };
    let store = match open() {
        Ok(store) => store,
        Err(e) => return fail(&e),
    };
    let corpus_id = match corpus_of(&store, args) {
        Ok(id) => id,
        Err(e) => return fail(&e),
    };
    if flag(args, "--count") {
        return match memory::counts(store.conn(), corpus_id) {
            Ok(rows) => {
                if flag(args, "--json") {
                    let items: Vec<Value> = rows
                        .iter()
                        .map(|(k, s, n)| json!({"kind": k, "status": s, "count": n}))
                        .collect();
                    println!("{}", pretty(&json!({"counts": items})));
                } else {
                    print_counts(&rows);
                }
                ExitCode::SUCCESS
            }
            Err(e) => fail(&e),
        };
    }
    let filter = Filter {
        kind,
        status,
        corpus_id,
        anchor: None,
        include_superseded: true,
        limit: if limit == 0 { None } else { Some(limit) },
    };
    let records = match memory::list(store.conn(), &filter) {
        Ok(records) => records,
        Err(e) => return fail(&e),
    };
    if flag(args, "--json") {
        println!("{}", pretty(&json!({"records": records})));
        return ExitCode::SUCCESS;
    }
    for r in &records {
        let place = r.anchor.clone().unwrap_or_else(|| r.source.clone());
        println!(
            "{:>6}  {:<9} {:<8} {:<10} {}  {}",
            r.id,
            r.kind,
            r.authority,
            r.status,
            clip(&r.title, 70),
            place
        );
    }
    println!("{} records", records.len());
    ExitCode::SUCCESS
}

pub fn recall(args: &[String]) -> ExitCode {
    if let Some(problem) = missing_value(args) {
        return fail(&problem);
    }
    let kind = match kind_of(args) {
        Ok(kind) => kind,
        Err(e) => return fail(&e),
    };
    let limit = match limit_of(args, 5) {
        Ok(n) => n.max(1),
        Err(e) => return fail(&e),
    };
    let text = positionals(args).join(" ");
    let anchor = option(args, "--anchor");
    if text.trim().is_empty() && anchor.is_none() {
        return usage("recall <query> [--anchor path#symbol] [--kind K] [--corpus C] [--limit N] [--json]");
    }
    let store = match open() {
        Ok(store) => store,
        Err(e) => return fail(&e),
    };
    let corpus_id = match corpus_of(&store, args) {
        Ok(id) => id,
        Err(e) => return fail(&e),
    };
    let query = recall::Query { text: text.clone(), anchor: anchor.clone(), kind, corpus_id, limit };
    let found = match recall::recall(store.conn(), &query) {
        Ok(found) => found,
        Err(e) => return fail(&e),
    };
    if flag(args, "--json") {
        println!("{}", pretty(&json!(found)));
        return ExitCode::SUCCESS;
    }
    let what = match (&anchor, text.trim().is_empty()) {
        (Some(a), true) => format!("anchor {a}"),
        (Some(a), false) => format!("\"{}\" at anchor {a}", text.trim()),
        (None, _) => format!("\"{}\"", text.trim()),
    };
    println!("recall {what}: {} records", found.hits.len());
    if found.hits.is_empty() {
        println!("no records match");
        return ExitCode::SUCCESS;
    }
    print!("{}", found.rendered);
    if let Some(budget) = found.budget_tokens {
        if found.truncated > 0 || found.omitted > 0 {
            println!(
                "note: to stay within {budget} tokens, {} longest texts were truncated (marked in the text) and {} records were left out",
                found.truncated, found.omitted
            );
        }
    }
    println!("total ~{} tokens (characters / 4, rounded up)", found.tokens);
    ExitCode::SUCCESS
}

pub fn remember(args: &[String]) -> ExitCode {
    if let Some(problem) = missing_value(args) {
        return fail(&problem);
    }
    let Some(text) = option(args, "--propose") else {
        return usage("remember --propose \"<text>\" [--anchor path#symbol] [--kind decision|how-to] [--corpus C] [--json]");
    };
    let kind = option(args, "--kind").unwrap_or_else(|| "decision".to_string());
    let anchor = option(args, "--anchor");
    let store = match open() {
        Ok(store) => store,
        Err(e) => return fail(&e),
    };
    let corpus_id = match corpus_of(&store, args) {
        Ok(id) => id,
        Err(e) => return fail(&e),
    };
    let proposal = match propose::propose(store.conn(), &text, anchor.as_deref(), &kind, corpus_id) {
        Ok(p) => p,
        Err(e) => return fail(&e),
    };
    if flag(args, "--json") {
        println!("{}", pretty(&json!(proposal)));
        return ExitCode::SUCCESS;
    }
    if proposal.created {
        println!("proposed record {} (status proposed, authority agent, kind {kind})", proposal.id);
    } else {
        println!("no new record: record {} already holds this text; a proposal event was added to it", proposal.id);
    }
    match proposal.target {
        Some(t) => println!("proposed action: {} record {t}", proposal.action),
        None => println!("proposed action: {}", proposal.action),
    }
    println!("{}", proposal.note);
    if proposal.redacted > 0 {
        println!("{} secret-shaped values were redacted before storing", proposal.redacted);
    }
    println!("nothing was deleted and no owner record was edited");
    ExitCode::SUCCESS
}

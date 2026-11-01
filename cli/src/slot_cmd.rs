use std::path::PathBuf;
use std::process::ExitCode;

use formslot_core::config;
use formslot_core::slots::consumer::{self, Applied, DriftSlot, InferRow, Plan};
use formslot_core::slots::{lock, manifest, refresh, seed, storage};
use formslot_core::store::Store;
use serde_json::{Value, json};

const VALUED: [&str; 5] = ["--status", "--corpus", "--repo", "--reason", "--format"];

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
    eprintln!("usage: formslot slot {line}");
    ExitCode::from(2)
}

fn open() -> Result<Store, String> {
    Store::open(&config::corpus_db())
}

fn short(hash: &str) -> String {
    if hash.is_empty() { "-".to_string() } else { hash.chars().take(10).collect() }
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}

fn repo_of(args: &[String]) -> Result<PathBuf, String> {
    let given = option(args, "--repo").ok_or_else(|| "--repo <path> is required".to_string())?;
    let path = PathBuf::from(given);
    if !path.is_dir() {
        return Err(format!("{} is not a folder", path.display()));
    }
    path.canonicalize().map_err(|e| format!("cannot open {}: {e}", path.display()))
}

pub fn run(args: &[String]) -> ExitCode {
    if let Some(problem) = missing_value(args) {
        return fail(&problem);
    }
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    match args.first().map(String::as_str) {
        Some("list") => list(rest),
        Some("get") => get(rest),
        Some("seed") => seed_cmd(rest),
        Some("refresh") => refresh_cmd(rest),
        Some("propose") => propose(rest),
        Some("confirm") => confirm(rest),
        Some("decline") => decline(rest),
        Some("plan") => plan(rest),
        Some("apply") => apply(rest),
        Some("drift") => drift(rest),
        _ => usage(
            "list | get | seed | refresh | propose | confirm | decline | plan | apply | drift (see formslot help)",
        ),
    }
}

fn list(args: &[String]) -> ExitCode {
    let status = option(args, "--status");
    if let Some(s) = &status {
        if !manifest::STATUSES.contains(&s.as_str()) {
            return fail(&format!("--status must be one of {}", manifest::STATUSES.join(" | ")));
        }
    }
    let store = match open() {
        Ok(store) => store,
        Err(e) => return fail(&e),
    };
    let rows = match storage::list(store.conn(), status.as_deref()) {
        Ok(rows) => rows,
        Err(e) => return fail(&e),
    };
    if flag(args, "--json") {
        let list: Vec<Value> = rows
            .iter()
            .map(|r| json!({ "id": r.id, "status": r.status, "version": r.version, "contentHash": r.content_hash }))
            .collect();
        println!("{}", pretty(&json!({ "slots": list })));
        return ExitCode::SUCCESS;
    }
    if rows.is_empty() {
        println!("no slots{}", status.map(|s| format!(" with status {s}")).unwrap_or_default());
        return ExitCode::SUCCESS;
    }
    let width = rows.iter().map(|r| r.id.len()).max().unwrap_or(2).max(2);
    println!("{:<width$}  {:<10}  {:<8}  {:>5}  {:>6}  {}", "id", "status", "version", "files", "copies", "title");
    for row in &rows {
        let m = &row.manifest;
        let files = m.get("files").and_then(Value::as_array).map(Vec::len).unwrap_or(0);
        let copies = m.get("copies").and_then(Value::as_array).map(Vec::len).unwrap_or(0);
        println!(
            "{:<width$}  {:<10}  {:<8}  {:>5}  {:>6}  {}",
            row.id,
            row.status,
            row.version,
            files,
            copies,
            manifest::text_at(m, &["title"]).unwrap_or("")
        );
    }
    println!("{} slot(s)", rows.len());
    ExitCode::SUCCESS
}

fn get(args: &[String]) -> ExitCode {
    let ids = positionals(args);
    let Some(id) = ids.first() else {
        return usage("get <id> [--json]");
    };
    let store = match open() {
        Ok(store) => store,
        Err(e) => return fail(&e),
    };
    let row = match storage::must_get(store.conn(), id) {
        Ok(row) => row,
        Err(e) => return fail(&e),
    };
    if flag(args, "--json") {
        println!("{}", pretty(&row.manifest));
        return ExitCode::SUCCESS;
    }
    let m = &row.manifest;
    let text = |keys: &[&str]| manifest::text_at(m, keys).unwrap_or("").to_string();
    println!("{}  {}  v{}  {}", row.id, row.status, row.version, text(&["kind"]));
    println!("{}", text(&["title"]));
    println!("{}", text(&["summary"]));
    println!();
    println!(
        "origin    {} {}  {} {}  commit {}",
        text(&["origin", "app"]),
        text(&["origin", "appVersion"]),
        text(&["origin", "repo"]),
        text(&["origin", "path"]),
        short(&text(&["origin", "commit"]))
    );
    println!("hash      {}", if row.content_hash.is_empty() { "-".to_string() } else { row.content_hash.clone() });
    println!("updated   {}", row.updated_at);
    if let Some(reason) = m.get("statusReason").and_then(Value::as_str) {
        println!("reason    {reason}");
    }
    println!();
    println!("files");
    for f in manifest::files_of(m) {
        println!(
            "  {}  {}  {}  {} lines  sha256 {}",
            f.path,
            f.role,
            f.port,
            f.lines.map(|n| n.to_string()).unwrap_or_else(|| "-".to_string()),
            short(&f.sha256)
        );
    }
    let deps = manifest::depends_on(m);
    if !deps.is_empty() {
        println!();
        println!("depends on  {}", deps.join(", "));
    }
    let copies = m.get("copies").and_then(Value::as_array).cloned().unwrap_or_default();
    println!();
    println!("copies ({})", copies.len());
    for c in &copies {
        let lines = c.get("lines").and_then(Value::as_array).cloned().unwrap_or_default();
        println!(
            "  {:<18} {:>5.3}  {}  {}#{}  {}-{}",
            c.get("relation").and_then(Value::as_str).unwrap_or(""),
            c.get("score").and_then(Value::as_f64).unwrap_or(0.0),
            c.get("repo").and_then(Value::as_str).unwrap_or(""),
            c.get("path").and_then(Value::as_str).unwrap_or(""),
            c.get("qualified").and_then(Value::as_str).unwrap_or(""),
            lines.first().and_then(Value::as_i64).unwrap_or(0),
            lines.get(1).and_then(Value::as_i64).unwrap_or(0)
        );
    }
    if let Ok(events) = storage::events(store.conn(), id) {
        println!();
        println!("events ({})", events.len());
        for e in events {
            println!("  {}  {:<8} {:<6} {}", e.at, e.action, e.actor, e.note);
        }
    }
    ExitCode::SUCCESS
}

fn seed_cmd(args: &[String]) -> ExitCode {
    let dirs = positionals(args);
    let Some(dir) = dirs.first() else {
        return usage("seed <dir>");
    };
    let mut store = match open() {
        Ok(store) => store,
        Err(e) => return fail(&e),
    };
    let report = match seed::seed(&mut store, &PathBuf::from(dir)) {
        Ok(report) => report,
        Err(e) => return fail(&e),
    };
    if flag(args, "--json") {
        println!("{}", pretty(&serde_json::to_value(&report).unwrap_or(Value::Null)));
    } else {
        for line in &report.lines {
            let id = if line.id.is_empty() { "-" } else { &line.id };
            if line.detail.is_empty() {
                println!("{:<10} {}  ({})", line.outcome, id, line.file);
            } else {
                println!("{:<10} {}  ({}): {}", line.outcome, id, line.file, line.detail);
            }
        }
        println!(
            "seed: {} imported, {} unchanged, {} skipped, {} rejected",
            report.imported, report.unchanged, report.skipped, report.rejected
        );
    }
    if report.rejected > 0 { ExitCode::from(1) } else { ExitCode::SUCCESS }
}

fn refresh_cmd(args: &[String]) -> ExitCode {
    let Some(corpus) = option(args, "--corpus") else {
        return usage("refresh <id|--all> --corpus <name>");
    };
    let mut store = match open() {
        Ok(store) => store,
        Err(e) => return fail(&e),
    };
    let ids: Vec<String> = if flag(args, "--all") {
        match storage::list(store.conn(), None) {
            Ok(rows) => rows
                .into_iter()
                .filter(|r| {
                    manifest::text_at(&r.manifest, &["origin", "repo"]) == Some(corpus.as_str())
                        && matches!(r.status.as_str(), "proposed" | "canonical" | "deprecated")
                })
                .map(|r| r.id)
                .collect(),
            Err(e) => return fail(&e),
        }
    } else {
        let given = positionals(args);
        if given.is_empty() {
            return usage("refresh <id|--all> --corpus <name>");
        }
        given
    };
    if ids.is_empty() {
        println!("no slot has its origin in {corpus}");
        return ExitCode::SUCCESS;
    }
    let mut failed = false;
    for id in ids {
        match refresh::refresh(&mut store, &id, &corpus) {
            Ok(r) => {
                if flag(args, "--json") {
                    println!("{}", pretty(&serde_json::to_value(&r).unwrap_or(Value::Null)));
                } else {
                    println!(
                        "refreshed {}  corpus {} at {}  {} file(s), {} changed, {} copies  contentHash {}{}",
                        r.id,
                        r.corpus,
                        short(&r.commit),
                        r.files,
                        r.changed_files,
                        r.copies,
                        short(&r.content_hash),
                        if r.content_hash_changed { " (new)" } else { " (same)" }
                    );
                }
            }
            Err(e) => {
                eprintln!("formslot: {id}: {e}");
                failed = true;
            }
        }
    }
    if failed { ExitCode::from(1) } else { ExitCode::SUCCESS }
}

fn propose(args: &[String]) -> ExitCode {
    let files = positionals(args);
    let Some(file) = files.first() else {
        return usage("propose <file.json>");
    };
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(e) => return fail(&format!("cannot read {file}: {e}")),
    };
    let value = match manifest::parse(&text) {
        Ok(value) => value,
        Err(e) => return fail(&format!("{file}: {e}")),
    };
    let mut store = match open() {
        Ok(store) => store,
        Err(e) => return fail(&e),
    };
    match storage::propose(&mut store, &value) {
        Ok(how) => {
            println!("{} {}  status proposed; the owner confirms with: formslot slot confirm {} --owner", how, manifest::id_of(&value), manifest::id_of(&value));
            ExitCode::SUCCESS
        }
        Err(e) => fail(&e),
    }
}

fn confirm(args: &[String]) -> ExitCode {
    let ids = positionals(args);
    let Some(id) = ids.first() else {
        return usage("confirm <id> --owner");
    };
    if !flag(args, "--owner") {
        return fail("refused: only the owner makes a slot canonical; run again with --owner if you are the owner");
    }
    let mut store = match open() {
        Ok(store) => store,
        Err(e) => return fail(&e),
    };
    match storage::confirm(&mut store, id) {
        Ok(row) => {
            println!("{} is now {}", row.id, row.status);
            ExitCode::SUCCESS
        }
        Err(e) => fail(&e),
    }
}

fn decline(args: &[String]) -> ExitCode {
    let ids = positionals(args);
    let Some(id) = ids.first() else {
        return usage("decline <id> --owner --reason \"...\"");
    };
    if !flag(args, "--owner") {
        return fail("refused: only the owner declines a slot; run again with --owner if you are the owner");
    }
    let Some(reason) = option(args, "--reason").filter(|r| !r.trim().is_empty()) else {
        return fail("a decline needs --reason \"...\"");
    };
    let mut store = match open() {
        Ok(store) => store,
        Err(e) => return fail(&e),
    };
    match storage::decline(&mut store, id, &reason) {
        Ok(row) => {
            println!("{} is now {} ({reason})", row.id, row.status);
            ExitCode::SUCCESS
        }
        Err(e) => fail(&e),
    }
}

fn print_plan(plan: &Plan) {
    println!("plan for {}", plan.repo);
    for (i, slot) in plan.slots.iter().enumerate() {
        let via = slot.pulled_in_by.as_ref().map(|p| format!("  (needed by {p})")).unwrap_or_default();
        println!("{}. {}  v{}  {}{}", i + 1, slot.id, slot.version, slot.status, via);
        if !slot.depends_on.is_empty() {
            println!("   dependsOn {}", slot.depends_on.join(", "));
        }
        for f in &slot.files {
            let state = if f.exists { "exists" } else { "missing" };
            println!(
                "   {} -> {}  [{}, by {}]  {}  {}",
                f.origin_path, f.target, state, f.resolved_by, f.role, f.port
            );
        }
    }
    for w in &plan.warnings {
        println!("note: {w}");
    }
}

fn plan(args: &[String]) -> ExitCode {
    let repo = match repo_of(args) {
        Ok(repo) => repo,
        Err(e) => return fail(&e),
    };
    let ids = positionals(args);
    let store = match open() {
        Ok(store) => store,
        Err(e) => return fail(&e),
    };
    match consumer::plan(store.conn(), &repo, &ids, flag(args, "--force")) {
        Ok(plan) => {
            if flag(args, "--json") {
                println!("{}", pretty(&serde_json::to_value(&plan).unwrap_or(Value::Null)));
            } else {
                print_plan(&plan);
            }
            ExitCode::SUCCESS
        }
        Err(e) => fail(&e),
    }
}

fn print_applied(applied: &Applied) {
    print_plan(&applied.plan);
    for note in &applied.notes {
        println!("note: {note}");
    }
    if applied.wrote {
        println!("wrote {}", applied.lockfile);
    } else {
        println!("dry run: {} not written", applied.lockfile);
    }
}

fn apply(args: &[String]) -> ExitCode {
    let repo = match repo_of(args) {
        Ok(repo) => repo,
        Err(e) => return fail(&e),
    };
    let ids = positionals(args);
    let mut store = match open() {
        Ok(store) => store,
        Err(e) => return fail(&e),
    };
    match consumer::apply(&mut store, &repo, &ids, flag(args, "--force"), flag(args, "--dry-run")) {
        Ok(applied) => {
            if flag(args, "--json") {
                println!("{}", pretty(&serde_json::to_value(&applied).unwrap_or(Value::Null)));
            } else {
                print_applied(&applied);
            }
            ExitCode::SUCCESS
        }
        Err(e) => fail(&e),
    }
}

fn print_lock_drift(slots: &[DriftSlot]) {
    for slot in slots {
        let version = match &slot.current_version {
            Some(v) if *v != slot.lock_version => format!("v{} (slot is now v{v})", slot.lock_version),
            _ => format!("v{}", slot.lock_version),
        };
        println!("{}  {}  {}", slot.id, version, slot.state);
        for f in &slot.files {
            let lines = format!(
                "installed {} lines, origin {} lines",
                f.installed_lines.map(|n| n.to_string()).unwrap_or_else(|| "-".to_string()),
                f.origin_lines.map(|n| n.to_string()).unwrap_or_else(|| "-".to_string())
            );
            let note = if f.note.is_empty() { String::new() } else { format!("  ({})", f.note) };
            println!("  {:<9} {}  <- {}  {}{}", f.state, f.path, f.origin_path, lines, note);
        }
    }
}

fn print_infer(rows: &[InferRow]) {
    if rows.is_empty() {
        println!("no file in the repository matches the name of a slot file");
        return;
    }
    let width = rows.iter().map(|r| r.file.len()).max().unwrap_or(4);
    println!("{:<width$}  {:<10}  {:<24}  lines here  lines slot", "file", "state", "slot");
    for r in rows {
        println!(
            "{:<width$}  {:<10}  {:<24}  {:>10}  {:>10}   {}",
            r.file,
            r.state,
            r.slot,
            r.file_lines,
            r.slot_lines.map(|n| n.to_string()).unwrap_or_else(|| "-".to_string()),
            r.slot_path
        );
    }
    let identical = rows.iter().filter(|r| r.state == "identical").count();
    let drifted = rows.iter().filter(|r| r.state == "drifted").count();
    println!("{} file(s) matched: {identical} identical, {drifted} drifted", rows.len());
}

fn drift(args: &[String]) -> ExitCode {
    let repo = match repo_of(args) {
        Ok(repo) => repo,
        Err(e) => return fail(&e),
    };
    let store = match open() {
        Ok(store) => store,
        Err(e) => return fail(&e),
    };
    let json_out = flag(args, "--json");
    let check = flag(args, "--check");
    let lockfile = match lock::read(&repo) {
        Ok(found) => found,
        Err(e) => return fail(&e),
    };
    match (lockfile, flag(args, "--infer")) {
        (Some(found), false) => match consumer::drift_lock(store.conn(), &repo, &found) {
            Ok(slots) => {
                if json_out {
                    println!("{}", pretty(&serde_json::to_value(&slots).unwrap_or(Value::Null)));
                } else {
                    println!("{} in {}: {} slot(s)", lock::FILE_NAME, repo.display(), slots.len());
                    print_lock_drift(&slots);
                }
                if check && slots.iter().any(|s| s.state != "clean") {
                    return ExitCode::from(1);
                }
                ExitCode::SUCCESS
            }
            Err(e) => fail(&e),
        },
        (None, true) => match consumer::drift_infer(store.conn(), &repo) {
            Ok(rows) => {
                if json_out {
                    println!("{}", pretty(&serde_json::to_value(&rows).unwrap_or(Value::Null)));
                } else {
                    println!("no {} in {}; matching files by name", lock::FILE_NAME, repo.display());
                    print_infer(&rows);
                }
                if check && rows.iter().any(|r| r.state != "identical") {
                    return ExitCode::from(1);
                }
                ExitCode::SUCCESS
            }
            Err(e) => fail(&e),
        },
        (Some(_), true) => fail(&format!(
            "{} exists in {}; --infer is for a repository with no lockfile",
            lock::FILE_NAME,
            repo.display()
        )),
        (None, false) => fail(&format!(
            "no {} in {}; run slot apply to write one, or slot drift --infer to match files by name",
            lock::FILE_NAME,
            repo.display()
        )),
    }
}

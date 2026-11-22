use std::path::Path;
use std::process::ExitCode;

use formslot_core::config;
use formslot_core::slots::suggest::{self, Options};
use formslot_core::store::Store;

const USAGE: &str = "usage: formslot slot suggest <corpus> [--limit N] [--min-copies K] [--json] [--write DIR]";

fn fail(message: &str) -> ExitCode {
    eprintln!("formslot: {message}");
    ExitCode::from(1)
}

fn usage() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::from(2)
}

fn number(args: &[String], at: usize, name: &str) -> Result<usize, String> {
    args.get(at)
        .and_then(|v| v.parse::<usize>().ok())
        .ok_or_else(|| format!("{name} needs a number"))
}

pub fn run(args: &[String]) -> ExitCode {
    let mut corpus: Option<String> = None;
    let mut opts = Options { limit: suggest::DEFAULT_LIMIT, min_copies: suggest::DEFAULT_MIN_COPIES };
    let mut json_out = false;
    let mut write: Option<String> = None;
    let mut at = 0;
    while at < args.len() {
        match args[at].as_str() {
            "--json" => json_out = true,
            "--limit" => {
                at += 1;
                match number(args, at, "--limit") {
                    Ok(n) => opts.limit = n,
                    Err(e) => return fail(&e),
                }
            }
            "--min-copies" => {
                at += 1;
                match number(args, at, "--min-copies") {
                    Ok(n) if n >= 1 => opts.min_copies = n,
                    _ => return fail("--min-copies needs a number of at least 1"),
                }
            }
            "--write" => {
                at += 1;
                match args.get(at) {
                    Some(v) if !v.starts_with("--") => write = Some(v.clone()),
                    _ => return fail("--write needs a folder"),
                }
            }
            other if other.starts_with("--") => return usage(),
            other => {
                if corpus.is_some() {
                    return usage();
                }
                corpus = Some(other.to_string());
            }
        }
        at += 1;
    }
    let Some(corpus) = corpus else { return usage() };
    let store = match Store::open(&config::corpus_db()) {
        Ok(store) => store,
        Err(e) => return fail(&e),
    };
    let report = match suggest::suggest(&store, &corpus, opts) {
        Ok(report) => report,
        Err(e) => return fail(&e),
    };
    let mut written: Vec<String> = Vec::new();
    if let Some(dir) = &write {
        let dir = Path::new(dir);
        if let Err(e) = std::fs::create_dir_all(dir) {
            return fail(&format!("cannot create {}: {e}", dir.display()));
        }
        for s in &report.suggestions {
            let file = dir.join(format!("{}.json", s.id));
            if std::fs::symlink_metadata(&file).is_ok_and(|m| m.file_type().is_symlink()) {
                return fail(&format!("{} is a symbolic link; not written", file.display()));
            }
            let text = serde_json::to_string_pretty(&s.manifest).unwrap_or_default();
            let opened = std::fs::OpenOptions::new().write(true).create_new(true).open(&file);
            let mut out = match opened {
                Ok(f) => f,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    eprintln!("kept {} (already there; delete it to draft it again)", file.display());
                    continue;
                }
                Err(e) => return fail(&format!("cannot write {}: {e}", file.display())),
            };
            if let Err(e) = std::io::Write::write_all(&mut out, format!("{text}\n").as_bytes()) {
                return fail(&format!("cannot write {}: {e}", file.display()));
            }
            written.push(file.display().to_string());
        }
    }
    if json_out {
        let value = suggest::report_json(&report, &written);
        println!("{}", serde_json::to_string_pretty(&value).unwrap_or_default());
        return ExitCode::SUCCESS;
    }
    println!(
        "slot suggestions for {} @ {} · {} candidates, {} files skipped as already in a slot · {} ms",
        report.corpus,
        report.commit.chars().take(12).collect::<String>(),
        report.candidates,
        report.skipped_existing,
        report.millis
    );
    for (i, s) in report.suggestions.iter().enumerate() {
        let files: Vec<String> = s.manifest["files"]
            .as_array()
            .map(|l| l.iter().filter_map(|f| f["path"].as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        println!("\n{:>2}. {}  score {:.1}  {}  {} file{}", i + 1, s.id, s.score, s.manifest["kind"].as_str().unwrap_or(""), files.len(), if files.len() == 1 { "" } else { "s" });
        println!("    {}", s.manifest["title"].as_str().unwrap_or(""));
        println!("    {}", s.manifest["summary"].as_str().unwrap_or(""));
        println!("    reason: {}", s.reason);
        println!("    files: {}", files.join(", "));
    }
    for line in &report.invalid {
        eprintln!("formslot: draft dropped, fails the schema: {line}");
    }
    if report.suggestions.is_empty() {
        println!("\nno candidates; --min-copies lowers the bar, formslot dupes {} shows the clone groups", report.corpus);
    }
    if !written.is_empty() {
        println!("\nwrote {} draft{}; review, then: formslot slot propose <file.json>", written.len(), if written.len() == 1 { "" } else { "s" });
    }
    ExitCode::SUCCESS
}

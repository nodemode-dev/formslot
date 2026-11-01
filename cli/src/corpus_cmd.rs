use std::path::PathBuf;
use std::process::ExitCode;

use formslot_core::corpus::{self, Corpus, IndexRun};
use formslot_core::store::Store;
use formslot_core::{config, family};

fn option(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn positional(args: &[String]) -> Option<String> {
    let mut skip = false;
    for arg in args {
        if skip {
            skip = false;
            continue;
        }
        if matches!(arg.as_str(), "--ref" | "--role" | "--language" | "--family-json") {
            skip = true;
            continue;
        }
        if !arg.starts_with("--") {
            return Some(arg.clone());
        }
    }
    None
}

fn open() -> Result<Store, String> {
    Store::open(&config::corpus_db())
}

fn fail(message: String) -> ExitCode {
    eprintln!("formslot: {message}");
    ExitCode::FAILURE
}

fn report(found: &Corpus, run: &IndexRun) {
    println!(
        "{} at {} · {} files listed, {} read, {} reused, {} skipped, {} secret findings, {} ms",
        found.name,
        found.head.as_deref().map(|h| h.get(..12).unwrap_or(h)).unwrap_or("no commit"),
        run.listed,
        run.read,
        run.reused,
        run.skipped,
        run.secrets,
        run.millis
    );
}

pub fn add(args: &[String]) -> ExitCode {
    let Some(path) = positional(args) else {
        return fail("add needs a path: formslot add <path> [--ref <rev>]".into());
    };
    let rev = option(args, "--ref").unwrap_or_else(|| "HEAD".into());
    let result = open().and_then(|mut store| corpus::add(&mut store, &PathBuf::from(path), &rev));
    match result {
        Ok((found, run)) => {
            report(&found, &run);
            ExitCode::SUCCESS
        }
        Err(e) => fail(e),
    }
}

pub fn reindex(args: &[String]) -> ExitCode {
    let Some(name) = positional(args) else {
        return fail("reindex needs a corpus name: formslot reindex <name> [--ref <rev>]".into());
    };
    let rev = option(args, "--ref").unwrap_or_else(|| "HEAD".into());
    match open().and_then(|mut store| corpus::reindex(&mut store, &name, &rev)) {
        Ok((found, run)) => {
            report(&found, &run);
            ExitCode::SUCCESS
        }
        Err(e) => fail(e),
    }
}

pub fn stats(args: &[String]) -> ExitCode {
    let Some(name) = positional(args) else {
        return fail("stats needs a corpus name: formslot stats <name> [--json]".into());
    };
    let stats = match open().and_then(|store| corpus::stats(&store, &name)) {
        Ok(stats) => stats,
        Err(e) => return fail(e),
    };
    if flag(args, "--json") {
        match serde_json::to_string_pretty(&stats) {
            Ok(text) => println!("{text}"),
            Err(e) => return fail(e.to_string()),
        }
        return ExitCode::SUCCESS;
    }
    let value = match serde_json::to_value(&stats) {
        Ok(value) => value,
        Err(e) => return fail(e.to_string()),
    };
    let rows = |key: &str| value.get(key).and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let text = |v: &serde_json::Value, key: &str| v.get(key).map(|x| x.to_string().trim_matches('"').to_string()).unwrap_or_default();
    println!("{:<12} {:>7} {:>9} {:>12}", "language", "files", "lines", "bytes");
    for row in rows("by_language") {
        println!("{:<12} {:>7} {:>9} {:>12}", text(&row, "language"), text(&row, "files"), text(&row, "lines"), text(&row, "bytes"));
    }
    println!();
    for row in rows("by_role") {
        println!("role {:<10} {:>7}", text(&row, "role"), text(&row, "files"));
    }
    println!();
    for row in rows("licences") {
        println!("licence {:<16} {:>7}", text(&row, "spdx"), text(&row, "files"));
    }
    println!("\nsecret findings: {}", text(&value, "secrets"));
    ExitCode::SUCCESS
}

pub fn files(args: &[String]) -> ExitCode {
    let Some(name) = positional(args) else {
        return fail("files needs a corpus name: formslot files <name> [--role <role>] [--language <lang>]".into());
    };
    let role = option(args, "--role");
    let language = option(args, "--language");
    match open().and_then(|store| corpus::files(&store, &name, role.as_deref(), language.as_deref())) {
        Ok(rows) => {
            for row in rows {
                match serde_json::to_value(&row) {
                    Ok(value) => println!(
                        "{}",
                        value.get("path").and_then(|p| p.as_str()).unwrap_or_default()
                    ),
                    Err(e) => return fail(e.to_string()),
                }
            }
            ExitCode::SUCCESS
        }
        Err(e) => fail(e),
    }
}

pub fn family(args: &[String]) -> ExitCode {
    let Some(source) = option(args, "--family-json") else {
        return fail("family needs its source: formslot family --family-json <formMCP core/data/family.json> [--check]".into());
    };
    let apps = match family::load_family_json(&PathBuf::from(source)) {
        Ok(apps) => apps,
        Err(e) => return fail(e),
    };
    let mut store = match open() {
        Ok(store) => store,
        Err(e) => return fail(e),
    };
    if flag(args, "--check") {
        return match family::check(&store, &apps) {
            Ok(true) => {
                println!("family list matches ({} apps)", apps.len());
                ExitCode::SUCCESS
            }
            Ok(false) => fail(format!("family list drifted from its source ({} apps there); run formslot family --family-json <path>", apps.len())),
            Err(e) => fail(e),
        };
    }
    match family::write_family(&mut store, &apps) {
        Ok(()) => {
            println!("family list written ({} apps)", apps.len());
            ExitCode::SUCCESS
        }
        Err(e) => fail(e),
    }
}

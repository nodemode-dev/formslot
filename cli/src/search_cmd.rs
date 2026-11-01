use std::process::ExitCode;
use std::time::Instant;

use formslot_core::config;
use formslot_core::search::{self, map, pack};
use formslot_core::store::Store;
use serde_json::json;

fn option(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn positionals(args: &[String], valued: &[&str]) -> Vec<String> {
    let mut found = Vec::new();
    let mut skip = false;
    for arg in args {
        if skip {
            skip = false;
            continue;
        }
        if valued.contains(&arg.as_str()) {
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

fn open() -> Result<Store, String> {
    Store::open(&config::corpus_db())
}

fn number(args: &[String], name: &str) -> Result<Option<usize>, String> {
    match option(args, name) {
        Some(text) => text
            .parse::<usize>()
            .map(Some)
            .map_err(|_| format!("{name} needs a whole number, not {text}")),
        None => Ok(None),
    }
}

pub fn run(args: &[String]) -> ExitCode {
    let valued = ["--corpus", "--limit"];
    let given = positionals(args, &valued);
    if given.len() == 1 && given[0] == "rebuild" {
        return rebuild(args);
    }
    if given.is_empty() {
        return fail("search needs a query, or `search rebuild [--corpus C]`".to_string());
    }
    let query = given.join(" ");
    let limit = match number(args, "--limit") {
        Ok(v) => v.unwrap_or(20),
        Err(e) => return fail(e),
    };
    let mut store = match open() {
        Ok(s) => s,
        Err(e) => return fail(e),
    };
    let started = Instant::now();
    let found = if flag(args, "--dense") {
        crate::dense_cmd::search(&mut store, &query, option(args, "--corpus").as_deref(), limit)
    } else {
        search::run(&mut store, &query, option(args, "--corpus").as_deref(), limit)
    };
    let hits = match found {
        Ok(h) => h,
        Err(e) => return fail(e),
    };
    if flag(args, "--json") {
        let value = json!({ "query": query, "millis": started.elapsed().as_millis() as u64, "hits": hits });
        println!("{}", serde_json::to_string_pretty(&value).unwrap_or_default());
        return ExitCode::SUCCESS;
    }
    for (i, h) in hits.iter().enumerate() {
        let place = match h.lines {
            Some([s, e]) => format!("{}:{s}-{e}", h.path),
            None => h.path.clone(),
        };
        println!("{:>3}  {:<6} {:.5}  {}  {}", i + 1, h.kind, h.score, place, h.id);
        println!("       {}", h.why.join("; "));
    }
    eprintln!("{} hits in {} ms", hits.len(), started.elapsed().as_millis());
    ExitCode::SUCCESS
}

fn rebuild(args: &[String]) -> ExitCode {
    let mut store = match open() {
        Ok(s) => s,
        Err(e) => return fail(e),
    };
    let started = Instant::now();
    match search::rebuild(&mut store, option(args, "--corpus").as_deref()) {
        Ok(done) => {
            for (name, stats) in done {
                println!("{name}: {} files, {} units indexed", stats.files, stats.added);
            }
            println!("memory index rebuilt, {} ms", started.elapsed().as_millis());
            ExitCode::SUCCESS
        }
        Err(e) => fail(e),
    }
}

pub fn map(args: &[String]) -> ExitCode {
    let valued = ["--tokens", "--corpus"];
    let given = positionals(args, &valued);
    let Some(key) = given.first() else {
        return fail("map needs a corpus name".to_string());
    };
    let tokens = match number(args, "--tokens") {
        Ok(v) => v,
        Err(e) => return fail(e),
    };
    let store = match open() {
        Ok(s) => s,
        Err(e) => return fail(e),
    };
    let target = match search::resolve_one(store.conn(), Some(key)) {
        Ok(t) => t,
        Err(e) => return fail(e),
    };
    if let Err(e) = search::ensure(store.conn(), std::slice::from_ref(&target)) {
        return fail(e);
    }
    let started = Instant::now();
    if flag(args, "--formmap") {
        return match map::formmap(store.conn(), &target, tokens.unwrap_or(map::FORMMAP_TOKENS)) {
            Ok(text) => {
                print!("{text}");
                eprintln!(
                    "FORMMAP {} lines, ~{} tokens, {} ms",
                    text.lines().count(),
                    search::tokens::code_text(&text),
                    started.elapsed().as_millis()
                );
                ExitCode::SUCCESS
            }
            Err(e) => fail(e),
        };
    }
    match map::map_text(store.conn(), &target, tokens.unwrap_or(1024)) {
        Ok((text, shown, total)) => {
            print!("{text}");
            eprintln!(
                "{shown} of {total} entries, ~{} tokens, {} ms",
                search::tokens::code_text(&text),
                started.elapsed().as_millis()
            );
            ExitCode::SUCCESS
        }
        Err(e) => fail(e),
    }
}

pub fn pack(args: &[String]) -> ExitCode {
    let valued = ["--corpus", "--tokens", "--tier", "--count"];
    let given = positionals(args, &valued);
    if given.is_empty() {
        return fail("pack needs a task in quotes".to_string());
    }
    let task = given.join(" ");
    let tokens = match number(args, "--tokens") {
        Ok(v) => v,
        Err(e) => return fail(e),
    };
    let tier = match option(args, "--tier") {
        Some(t) => match pack::Tier::parse(&t) {
            Some(v) => Some(v),
            None => return fail(format!("--tier is T1, T4 or T16, not {t}")),
        },
        None => None,
    };
    let remote = match option(args, "--count").as_deref() {
        None | Some("local") => false,
        Some("remote") => true,
        Some(other) => return fail(format!("--count is local or remote, not {other}")),
    };
    let api_key = if remote {
        match formslot_core::count::key() {
            Ok(k) => Some(k),
            Err(e) => return fail(e),
        }
    } else {
        None
    };
    let mut store = match open() {
        Ok(s) => s,
        Err(e) => return fail(e),
    };
    let started = Instant::now();
    let built = match pack::run(&mut store, &task, option(args, "--corpus").as_deref(), tokens, tier) {
        Ok(p) => p,
        Err(e) => return fail(e),
    };
    let millis = started.elapsed().as_millis() as u64;
    let counted = if let Some(key) = &api_key {
        match formslot_core::count::remote(key, &built.prompt) {
            Ok(n) => Some(n),
            Err(e) => {
                eprintln!("formslot: {e}; using the local estimate");
                None
            }
        }
    } else {
        None
    };
    if flag(args, "--json") {
        let mut value = serde_json::to_value(&built).unwrap_or_default();
        value["millis"] = json!(millis);
        if let Some(n) = counted {
            value["remoteTokens"] = json!(n);
        }
        value["prompt"] = json!(built.prompt);
        println!("{}", serde_json::to_string_pretty(&value).unwrap_or_default());
        return ExitCode::SUCCESS;
    }
    print!("{}", built.prompt);
    eprintln!("---");
    eprintln!(
        "pack {} tier {} budget {} tokens (estimate: {})",
        built.corpus, built.tier, built.budget, built.estimator
    );
    eprintln!("P0 sha256 {}", built.p0_sha256);
    if let Some(n) = counted {
        eprintln!("remote count (Messages count_tokens): {n} tokens");
    }
    eprintln!(
        "tokens P0 {}{}  P1 {}  P2 {}  sent {}  ({} ms)",
        built.tokens.p0,
        if built.parts.p0.included { "" } else { " (not sent in T1)" },
        built.tokens.p1,
        built.tokens.p2,
        built.tokens.sent,
        millis
    );
    ExitCode::SUCCESS
}

use std::process::ExitCode;

use formslot_core::config;
use formslot_core::slots::{manifest, source};
use formslot_core::store::Store;

fn fail(message: &str) -> ExitCode {
    eprintln!("formslot: {message}");
    ExitCode::from(1)
}

pub fn run(args: &[String]) -> ExitCode {
    let mut id: Option<String> = None;
    let mut file: Option<String> = None;
    let mut json = false;
    let mut at = 0;
    while at < args.len() {
        match args[at].as_str() {
            "--json" => json = true,
            "--file" => {
                at += 1;
                match args.get(at) {
                    Some(v) if !v.starts_with("--") => file = Some(v.clone()),
                    _ => return fail("--file needs a value"),
                }
            }
            other if other.starts_with("--") => {
                eprintln!("usage: formslot slot source <id> [--file PATH] [--json]");
                return ExitCode::from(2);
            }
            other => {
                if id.is_some() {
                    eprintln!("usage: formslot slot source <id> [--file PATH] [--json]");
                    return ExitCode::from(2);
                }
                id = Some(other.to_string());
            }
        }
        at += 1;
    }
    let Some(id) = id else {
        eprintln!("usage: formslot slot source <id> [--file PATH] [--json]");
        return ExitCode::from(2);
    };
    if !manifest::is_id(&id) {
        return fail(&format!("{id} is not a slot id (lowercase dotted kebab such as ui.control-rows)"));
    }
    let store = match Store::open(&config::corpus_db()) {
        Ok(store) => store,
        Err(e) => return fail(&e),
    };
    let value = match source::read(store.conn(), &id, file.as_deref(), source::Caps::STANDARD) {
        Ok(Some(value)) => value,
        Ok(None) => return fail(&format!("no slot {id}; formslot slot list shows the slots")),
        Err(e) => return fail(&e),
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&value).unwrap_or_default());
        return ExitCode::SUCCESS;
    }
    let text = |v: &serde_json::Value| v.as_str().unwrap_or("").to_string();
    println!(
        "slot {id} · {} · v{} · {} @ {} · licence {} · copy {}",
        text(&value["status"]),
        text(&value["version"]),
        text(&value["origin"]["repo"]),
        text(&value["commit"]).chars().take(12).collect::<String>(),
        text(&value["licence"]),
        text(&value["copy"])
    );
    println!("{}", text(&value["summary"]));
    let exports = value["api"]["exports"].as_array().cloned().unwrap_or_default();
    println!("exports ({}):", exports.len());
    for e in &exports {
        println!("  {} {}", text(&e["symbol"]), text(&e["signature"]));
    }
    let conventions = value["api"]["conventions"].as_array().cloned().unwrap_or_default();
    println!("conventions ({}):", conventions.len());
    for c in &conventions {
        println!("  [{}] {}: {}", text(&c["severity"]), text(&c["id"]), text(&c["text"]));
    }
    for f in value["files"].as_array().cloned().unwrap_or_default() {
        println!(
            "\n=== {} · {} bytes · {} · port {}{}{}",
            text(&f["source"]["path"]),
            f["bytes"],
            if f["matchesManifest"] == true { "matches manifest" } else { "differs from manifest hash" },
            text(&f["port"]),
            if f["truncated"] == true { " · TRUNCATED" } else { "" },
            if f["redactions"].as_u64().unwrap_or(0) > 0 { format!(" · {} redacted", f["redactions"]) } else { String::new() }
        );
        if let Some(note) = f["note"].as_str() {
            println!("({note})");
        }
        print!("{}", text(&f["text"]));
        if !text(&f["text"]).ends_with('\n') {
            println!();
        }
    }
    for o in value["omitted"].as_array().cloned().unwrap_or_default() {
        println!("\nomitted for the total size cap: {} ({} bytes); use --file", text(&o["path"]), o["bytes"]);
    }
    ExitCode::SUCCESS
}

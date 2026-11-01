use std::path::Path;
use std::process::ExitCode;

use formslot_core::config;
use formslot_core::eval::{self, Arm};
use formslot_core::store::Store;

fn option(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn fail(message: String) -> ExitCode {
    eprintln!("formslot: {message}");
    ExitCode::FAILURE
}

fn parse_ks(text: &str) -> Result<Vec<usize>, String> {
    let mut ks = Vec::new();
    for part in text.split(',') {
        let k = part
            .trim()
            .parse::<usize>()
            .map_err(|_| format!("--k is a list of whole numbers like 1,4,10, not {text}"))?;
        if k == 0 {
            return Err("--k values start at 1".to_string());
        }
        if !ks.contains(&k) {
            ks.push(k);
        }
    }
    ks.sort();
    Ok(ks)
}

pub fn run(args: &[String]) -> ExitCode {
    let gold_path = option(args, "--gold").unwrap_or_else(|| "docs/eval/gold.jsonl".to_string());
    let arms = match option(args, "--arm") {
        Some(text) => match Arm::parse(&text) {
            Some(a) => a,
            None => return fail(format!("--arm is formslot, baseline, dense or all, not {text}")),
        },
        None => vec![Arm::Formslot, Arm::Baseline, Arm::Dense],
    };
    let ks = match parse_ks(&option(args, "--k").unwrap_or_else(|| "1,4,10".to_string())) {
        Ok(k) => k,
        Err(e) => return fail(e),
    };
    let gold = match eval::load_gold(Path::new(&gold_path)) {
        Ok(g) => g,
        Err(e) => return fail(e),
    };
    let mut store = match Store::open(&config::corpus_db()) {
        Ok(s) => s,
        Err(e) => return fail(e),
    };
    let corpus = option(args, "--corpus");
    let report = match eval::run(&mut store, gold, &gold_path, corpus.as_deref(), &arms, &ks) {
        Ok(r) => r,
        Err(e) => return fail(e),
    };
    if flag(args, "--json") {
        println!("{}", serde_json::to_string_pretty(&eval::json(&report)).unwrap_or_default());
    } else {
        print!("{}", eval::table(&report));
        for line in eval::gates(&report).lines {
            println!("{line}");
        }
    }
    if let Some(out) = option(args, "--out") {
        let path = Path::new(&out);
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            return fail(format!("cannot create {}: {e}", parent.display()));
        }
        if let Err(e) = std::fs::write(path, eval::markdown(&report)) {
            return fail(format!("cannot write {out}: {e}"));
        }
        eprintln!("report written to {out}");
    }
    ExitCode::SUCCESS
}

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use formslot_core::check::Mode;
use formslot_core::{hook, install, log};
use serde_json::json;

fn option(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn stdin_text() -> String {
    let mut raw = Vec::new();
    let _ = std::io::stdin().lock().take(hook::STDIN_CAP as u64).read_to_end(&mut raw);
    String::from_utf8_lossy(&raw).to_string()
}

fn emit(out: Option<String>) -> ExitCode {
    if let Some(text) = out {
        let mut stdout = std::io::stdout().lock();
        let _ = writeln!(stdout, "{text}");
        let _ = stdout.flush();
    }
    ExitCode::SUCCESS
}

pub fn check(args: &[String]) -> ExitCode {
    let mode = option(args, "--mode");
    emit(hook::run_check(&stdin_text(), mode.as_deref()))
}

pub fn event(args: &[String]) -> ExitCode {
    let Some(name) = args.first() else {
        log::record("hook_error", json!({ "error": "no event named" }));
        return ExitCode::SUCCESS;
    };
    let Some(known) = hook::EVENTS.iter().find(|e| e.eq_ignore_ascii_case(name)) else {
        log::record("hook_error", json!({ "error": format!("unknown event {name}") }));
        return ExitCode::SUCCESS;
    };
    emit(hook::run_event(known, &stdin_text()))
}

pub fn install(args: &[String]) -> ExitCode {
    let mut skip = false;
    let mut repo: Option<String> = None;
    for a in args {
        if skip {
            skip = false;
            continue;
        }
        if a == "--mode" {
            skip = true;
        } else if !a.starts_with("--") && repo.is_none() {
            repo = Some(a.clone());
        }
    }
    let Some(repo) = repo else {
        eprintln!("formslot: usage: formslot install <repo> [--mode warn|deny] [--enable] [--dry-run]");
        return ExitCode::from(2);
    };
    let mode = match option(args, "--mode") {
        None => Mode::Warn,
        Some(m) => match Mode::parse(&m) {
            Some(Mode::Off) | None => {
                eprintln!("formslot: --mode must be warn or deny");
                return ExitCode::from(2);
            }
            Some(mode) => mode,
        },
    };
    let path = PathBuf::from(repo);
    let path = match path.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("formslot: cannot open {}: {e}", path.display());
            return ExitCode::FAILURE;
        }
    };
    match install::install(&path, mode, flag(args, "--enable"), flag(args, "--dry-run")) {
        Ok(report) => {
            for line in report.lines {
                println!("{line}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("formslot: {e}");
            ExitCode::FAILURE
        }
    }
}

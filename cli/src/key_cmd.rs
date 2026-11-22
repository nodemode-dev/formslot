use std::io::{BufRead, IsTerminal};
use std::process::ExitCode;

use formslot_core::keys::{self, ANTHROPIC, ANTHROPIC_ENV, Place};
use serde_json::json;

const USAGE: &str = "formslot key set      save the Anthropic key that `pack --count remote` uses; the key is read from standard input, never from the command line
formslot key remove   forget the saved key, in the system password store and in the file
formslot key status [--json]   whether a key is saved, where, and its last four characters; never reads the password store";

fn fail(message: String) -> ExitCode {
    eprintln!("formslot: {message}");
    ExitCode::FAILURE
}

#[cfg(unix)]
fn echo(on: bool) {
    let _ = std::process::Command::new("stty").arg(if on { "echo" } else { "-echo" }).status();
}

#[cfg(unix)]
fn read_hidden() -> std::io::Result<String> {
    let _ = ctrlc::set_handler(|| {
        echo(true);
        eprintln!();
        std::process::exit(130);
    });
    eprint!("Paste the Anthropic key and press Enter (it is not shown): ");
    echo(false);
    let mut line = String::new();
    let read = std::io::stdin().lock().read_line(&mut line);
    echo(true);
    eprintln!();
    read.map(|_| line)
}

#[cfg(not(unix))]
fn read_hidden() -> std::io::Result<String> {
    eprintln!("The key is shown as you paste it. To keep it off the screen, press Ctrl+C and pipe it in instead: formslot key set < file");
    eprint!("Paste the Anthropic key and press Enter: ");
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line).map(|_| line)
}

fn set() -> ExitCode {
    let read = if std::io::stdin().is_terminal() {
        read_hidden()
    } else {
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line).map(|_| line)
    };
    let line = match read {
        Ok(line) => line,
        Err(e) => return fail(format!("the key could not be read ({e})")),
    };
    match keys::set(ANTHROPIC, &line) {
        Ok(Place::System) => {
            println!("The Anthropic key is saved in the system password store.");
            ExitCode::SUCCESS
        }
        Ok(Place::File) => {
            println!(
                "This computer has no password store formslot can reach, so the Anthropic key is saved in a file only you can read: {}",
                keys::file_path(ANTHROPIC).display()
            );
            ExitCode::SUCCESS
        }
        Err(e) => fail(e),
    }
}

fn remove() -> ExitCode {
    match keys::remove(ANTHROPIC) {
        Ok(true) => {
            println!("The saved Anthropic key is removed.");
            ExitCode::SUCCESS
        }
        Ok(false) => {
            println!("No Anthropic key was saved.");
            ExitCode::SUCCESS
        }
        Err(e) => fail(e),
    }
}

fn status(args: &[String]) -> ExitCode {
    let status = keys::status(ANTHROPIC, ANTHROPIC_ENV);
    if args.iter().any(|a| a == "--json") {
        println!(
            "{}",
            json!({
                "name": ANTHROPIC,
                "saved": status.saved,
                "where": status.place.map(Place::id),
                "last4": status.last4,
                "env": status.env,
                "usedBy": ["pack --count remote"],
            })
        );
        return ExitCode::SUCCESS;
    }
    match status.place {
        Some(place) if status.last4.is_empty() => println!("Anthropic key: saved in {}.", place.words()),
        Some(place) => println!("Anthropic key: saved in {}, ending in {}.", place.words(), status.last4),
        None => println!("Anthropic key: not saved. Save one with: formslot key set"),
    }
    if status.env {
        println!("{ANTHROPIC_ENV} is set in this shell; it is used only when no key is saved.");
    }
    println!("The key is used by one command, `formslot pack --count remote`, which sends the pack text to api.anthropic.com to count its tokens.");
    ExitCode::SUCCESS
}

pub fn run(args: &[String]) -> ExitCode {
    match args.first().map(String::as_str) {
        Some("set") if args.len() == 1 => set(),
        Some("remove") if args.len() == 1 => remove(),
        Some("status") => status(&args[1..]),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

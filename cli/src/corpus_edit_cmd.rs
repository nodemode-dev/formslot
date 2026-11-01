use std::path::PathBuf;
use std::process::ExitCode;

use formslot_core::corpus;
use formslot_core::store::Store;
use formslot_core::config;

fn fail(message: String) -> ExitCode {
    eprintln!("formslot: {message}");
    ExitCode::FAILURE
}

fn open() -> Result<Store, String> {
    Store::open(&config::corpus_db())
}

fn option(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

pub fn add(args: &[String]) -> ExitCode {
    let Some(at) = args.iter().position(|a| a == "--name") else {
        return crate::corpus_cmd::add(args);
    };
    let Some(name) = args.get(at + 1).filter(|v| !v.starts_with("--")).cloned() else {
        return fail("--name needs a value".into());
    };
    let rest: Vec<String> = args
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != at && *i != at + 1)
        .map(|(_, a)| a.clone())
        .collect();
    let Some(path) = rest.iter().find(|a| !a.starts_with("--") && Some(a.as_str()) != option(&rest, "--ref").as_deref()).cloned() else {
        return fail("add needs a path: formslot add <path> [--name <name>] [--ref <rev>]".into());
    };
    let rev = option(&rest, "--ref").unwrap_or_else(|| "HEAD".into());
    match open().and_then(|mut store| corpus::add_named(&mut store, &PathBuf::from(path), &rev, Some(&name))) {
        Ok((found, run)) => {
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
            ExitCode::SUCCESS
        }
        Err(e) => fail(e),
    }
}

pub fn run(args: &[String]) -> ExitCode {
    match args.first().map(String::as_str) {
        Some("rename") => match (args.get(1), args.get(2), args.len()) {
            (Some(old), Some(new), 3) => match open().and_then(|mut store| corpus::rename(&mut store, old, new)) {
                Ok(done) => {
                    println!(
                        "{} renamed to {} · {} slot manifests updated, index, units and memory kept",
                        done.from, done.corpus.name, done.slots
                    );
                    ExitCode::SUCCESS
                }
                Err(e) => fail(e),
            },
            _ => fail("usage: formslot corpus rename <old> <new>".into()),
        },
        Some("move") => match (args.get(1), args.get(2), args.len()) {
            (Some(name), Some(path), 3) => {
                match open().and_then(|mut store| corpus::relocate(&mut store, name, &PathBuf::from(path))) {
                    Ok(found) => {
                        println!(
                            "{} now at {} · remote {} · run: formslot reindex {}",
                            found.name,
                            found.path,
                            found.remote.as_deref().unwrap_or("none"),
                            found.name
                        );
                        ExitCode::SUCCESS
                    }
                    Err(e) => fail(e),
                }
            }
            _ => fail("usage: formslot corpus move <name> <new-path>".into()),
        },
        Some("list") => match open().and_then(|store| corpus::list(&store)) {
            Ok(list) => {
                for c in list {
                    println!(
                        "{:<14} {} · {} · {}",
                        c.name,
                        c.head.as_deref().map(|h| h.get(..12).unwrap_or(h)).unwrap_or("not indexed"),
                        c.path,
                        c.remote.as_deref().unwrap_or("no remote")
                    );
                }
                ExitCode::SUCCESS
            }
            Err(e) => fail(e),
        },
        _ => fail("usage: formslot corpus rename <old> <new> | move <name> <new-path> | list".into()),
    }
}

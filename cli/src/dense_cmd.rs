use std::process::ExitCode;

use formslot_core::search::Hit;
use formslot_core::store::Store;

#[cfg(feature = "dense")]
fn flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

#[cfg(feature = "dense")]
fn option(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn fail(message: String) -> ExitCode {
    eprintln!("formslot: {message}");
    ExitCode::FAILURE
}

#[cfg(feature = "dense")]
pub fn search(store: &mut Store, query: &str, corpus: Option<&str>, limit: usize) -> Result<Vec<Hit>, String> {
    formslot_core::dense::run(store, query, corpus, limit)
}

#[cfg(not(feature = "dense"))]
pub fn search(_store: &mut Store, _query: &str, _corpus: Option<&str>, _limit: usize) -> Result<Vec<Hit>, String> {
    Err(unavailable())
}

#[cfg(not(feature = "dense"))]
fn unavailable() -> String {
    "this build has no dense tier; build with `cargo build --release --features dense`".to_string()
}

#[cfg(not(feature = "dense"))]
pub fn run(_args: &[String]) -> ExitCode {
    fail(unavailable())
}

#[cfg(feature = "dense")]
pub fn run(args: &[String]) -> ExitCode {
    use formslot_core::config;
    use formslot_core::dense::{self, fetch};

    let Some(sub) = args.first().map(String::as_str) else {
        return fail("dense needs fetch, build or status".to_string());
    };
    match sub {
        "fetch" => {
            print!("{}", fetch::plan());
            if !flag(args, "--yes") {
                return fail("nothing was downloaded; run `formslot dense fetch --yes` to download the files above".to_string());
            }
            match fetch::download() {
                Ok(notes) => {
                    for note in notes {
                        println!("{note}");
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => fail(e),
            }
        }
        "build" => {
            let store = match Store::open(&config::corpus_db()) {
                Ok(s) => s,
                Err(e) => return fail(e),
            };
            let started = std::time::Instant::now();
            let mut embedder = match dense::embed::Embedder::load() {
                Ok(e) => e,
                Err(e) => return fail(e),
            };
            let mut last = 0u64;
            let mut progress = |name: &str, done: u64, total: u64| {
                if done == total || done >= last + 512 {
                    eprintln!("{name}: {done} of {total} units embedded");
                    last = done;
                }
            };
            match dense::build(
                &store,
                &mut embedder,
                option(args, "--corpus").as_deref(),
                flag(args, "--rebuild"),
                &mut progress,
            ) {
                Ok(done) => {
                    for s in done {
                        println!(
                            "{}: {} units embedded, {} already done, {} skipped",
                            s.corpus, s.embedded, s.already, s.skipped
                        );
                    }
                    println!("{} ms", started.elapsed().as_millis());
                    ExitCode::SUCCESS
                }
                Err(e) => fail(e),
            }
        }
        "status" => {
            println!("model: {} ({})", dense::MODEL, if fetch::present() { "present" } else { "not fetched" });
            println!("model folder: {}", dense::model_dir().display());
            let store = match Store::open(&config::corpus_db()) {
                Ok(s) => s,
                Err(e) => return fail(e),
            };
            match dense::vec_version(store.conn()) {
                Ok(v) => println!("sqlite-vec: {v}"),
                Err(e) => println!("{e}"),
            }
            match dense::status(store.conn()) {
                Ok(rows) => {
                    for r in rows {
                        println!(
                            "{}: {} of {} units embedded{}",
                            r.corpus,
                            r.embedded,
                            r.units,
                            match (r.model, r.built_at) {
                                (Some(m), Some(t)) => format!(" ({m}, {t})"),
                                _ => String::new(),
                            }
                        );
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => fail(e),
            }
        }
        other => fail(format!("dense: unknown subcommand {other}; use fetch, build or status")),
    }
}

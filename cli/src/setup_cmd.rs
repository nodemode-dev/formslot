use std::ffi::OsString;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use formslot_core::git::Repo;
use formslot_core::setup::{self as engine, Need, Placement};
use formslot_core::store::Store;
use formslot_core::{VERSION, config, corpus};

static BUSY: AtomicBool = AtomicBool::new(false);
static INTERRUPTED: AtomicBool = AtomicBool::new(false);

const LEFT: &str = "Stopped. Nothing is left half-done: what you already finished is kept, and you can run `formslot setup` again to carry on.";

fn leave() -> ! {
    println!("\n{LEFT}");
    std::process::exit(130);
}

fn arm_interrupt() {
    let _ = ctrlc::set_handler(|| {
        INTERRUPTED.store(true, Ordering::SeqCst);
        if !BUSY.load(Ordering::SeqCst) {
            leave();
        }
    });
}

fn critical<T>(work: impl FnOnce() -> T) -> T {
    BUSY.store(true, Ordering::SeqCst);
    let out = work();
    BUSY.store(false, Ordering::SeqCst);
    if INTERRUPTED.load(Ordering::SeqCst) {
        leave();
    }
    out
}

struct Ui {
    color: bool,
    yes: bool,
}

impl Ui {
    fn new(yes: bool) -> Ui {
        let color = io::stdout().is_terminal()
            && std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty())
            && std::env::var("TERM").map(|t| t != "dumb").unwrap_or(true);
        Ui { color, yes }
    }

    fn paint(&self, code: &str, text: &str) -> String {
        if self.color { format!("\x1b[{code}m{text}\x1b[0m") } else { text.to_string() }
    }

    fn heading(&self, n: usize, title: &str, why: &str) {
        println!();
        println!("{}", self.paint("1", &format!("Step {n} of 6: {title}")));
        println!("  {}", self.paint("2", why));
    }

    fn tag(&self, kind: &str, text: &str) {
        let code = match kind {
            "done" | "found" => "32",
            "missing" | "problem" => "33",
            _ => "2",
        };
        println!("  {} {text}", self.paint(code, &format!("{kind:<8}")));
    }

    fn say(&self, text: &str) {
        println!("  {text}");
    }

    fn read(&self) -> String {
        let mut line = String::new();
        match io::stdin().lock().read_line(&mut line) {
            Ok(0) | Err(_) => leave(),
            Ok(_) => line.trim_end_matches(['\n', '\r']).to_string(),
        }
    }

    fn ask(&self, question: &str, default: Option<&str>) -> String {
        if self.yes {
            return default.unwrap_or("").to_string();
        }
        match default {
            Some(d) => print!("  {question} [{d}] "),
            None => print!("  {question} "),
        }
        let _ = io::stdout().flush();
        let answer = self.read();
        if answer.trim().is_empty() { default.unwrap_or("").to_string() } else { answer }
    }

    fn confirm(&self, question: &str, default_yes: bool) -> bool {
        if self.yes {
            return default_yes;
        }
        let shown = if default_yes { "Y/n" } else { "y/N" };
        loop {
            print!("  {question} [{shown}] ");
            let _ = io::stdout().flush();
            match self.read().trim().to_lowercase().as_str() {
                "" => return default_yes,
                "y" | "yes" => return true,
                "n" | "no" => return false,
                _ => println!("  Please answer y or n."),
            }
        }
    }
}

struct Options {
    yes: bool,
    dry: bool,
    no_path: bool,
    repos: Vec<String>,
}

fn parse(args: &[String]) -> Result<Options, String> {
    let mut o = Options { yes: false, dry: false, no_path: false, repos: Vec::new() };
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        match a {
            "--yes" | "-y" => o.yes = true,
            "--dry-run" => o.dry = true,
            "--no-path" => o.no_path = true,
            "--repo" => {
                i += 1;
                match args.get(i) {
                    Some(v) if !v.starts_with("--") => o.repos.push(v.clone()),
                    _ => return Err("--repo needs a folder".into()),
                }
            }
            _ if a.starts_with("--repo=") => o.repos.push(a["--repo=".len()..].to_string()),
            _ => return Err(format!("unknown option {a}")),
        }
        i += 1;
    }
    Ok(o)
}

fn indexed() -> Vec<(String, String, i64)> {
    let db = config::data_dir_path().join("corpus.sqlite");
    if !db.exists() {
        return Vec::new();
    }
    let Ok(conn) = rusqlite::Connection::open_with_flags(&db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) else {
        return Vec::new();
    };
    let Ok(mut stmt) = conn.prepare("SELECT name, path, files FROM corpus ORDER BY id") else {
        return Vec::new();
    };
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)));
    match rows {
        Ok(rows) => rows.flatten().collect(),
        Err(_) => Vec::new(),
    }
}

fn run_self(bin: &Path, args: &[&str], path_env: Option<&OsString>, own_group: bool) -> (bool, String) {
    let mut cmd = Command::new(bin);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(path) = path_env {
        cmd.env("PATH", path);
    }
    #[cfg(unix)]
    if own_group {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(not(unix))]
    let _ = own_group;
    match cmd.output() {
        Ok(out) => {
            let mut text = String::from_utf8_lossy(&out.stdout).to_string();
            text.push_str(&String::from_utf8_lossy(&out.stderr));
            (out.status.success(), text)
        }
        Err(e) => (false, e.to_string()),
    }
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or("").trim().to_string()
}

fn is_connected(bin: &Path, path: &str, path_env: Option<&OsString>) -> bool {
    let (ok, text) = run_self(bin, &["install", path, "--mode", "warn", "--dry-run"], path_env, false);
    ok && text.lines().filter(|l| l.ends_with(": nothing to change")).count() >= 2
}

fn quoted(name: &str) -> String {
    if name.chars().any(char::is_whitespace) { format!("\"{name}\"") } else { name.to_string() }
}

fn resolve(raw: &str) -> PathBuf {
    let cleaned = engine::clean_path(raw);
    let path = PathBuf::from(&cleaned);
    if path.is_absolute() {
        path
    } else {
        std::env::current_dir().map(|d| d.join(&path)).unwrap_or(path)
    }
}

struct Setup {
    ui: Ui,
    opts: Options,
    bin: PathBuf,
    path_dir: Option<PathBuf>,
    path_ok: bool,
    done: Vec<String>,
    kept: Vec<String>,
    skipped: Vec<String>,
    added: Vec<String>,
    failed: bool,
}

pub fn run(args: &[String]) -> ExitCode {
    let opts = match parse(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("formslot: {e}");
            eprintln!("usage: formslot setup [--yes] [--repo PATH]... [--no-path] [--dry-run]");
            return ExitCode::from(2);
        }
    };
    let interactive = io::stdin().is_terminal();
    if !interactive && !opts.yes && !opts.dry {
        eprintln!("formslot setup asks questions, so it needs a terminal to talk to.");
        eprintln!("To run it without questions, give it your answers up front:");
        eprintln!("  formslot setup --yes --repo /path/to/your/project");
        eprintln!("Add --no-path to leave your shell settings alone, or --dry-run to only see what would happen.");
        return ExitCode::from(2);
    }
    if interactive && !opts.yes && !opts.dry {
        arm_interrupt();
    }
    let exe = std::env::current_exe().and_then(|p| p.canonicalize()).unwrap_or_else(|_| PathBuf::from("formslot"));
    let mut s = Setup {
        ui: Ui::new(opts.yes || opts.dry),
        opts,
        bin: exe,
        path_dir: None,
        path_ok: false,
        done: Vec::new(),
        kept: Vec::new(),
        skipped: Vec::new(),
        added: Vec::new(),
        failed: false,
    };
    s.go()
}

impl Setup {
    fn go(&mut self) -> ExitCode {
        let dry = self.opts.dry;
        println!("{}", self.ui.paint("1", &format!("formslot {VERSION} setup")));
        println!("This gets formslot ready in six short steps. Press Enter to accept the answer in [brackets]. Press Ctrl+C at any time to stop; you can run setup again later and it will pick up where it left off.");
        if dry {
            println!("{}", self.ui.paint("33", "Dry run: nothing will be changed. This only says what would happen."));
        }
        if !self.step_check() {
            return ExitCode::FAILURE;
        }
        self.step_install();
        self.step_service();
        self.step_projects();
        self.step_connect();
        self.step_finish();
        if self.failed { ExitCode::FAILURE } else { ExitCode::SUCCESS }
    }

    fn word(&self) -> String {
        if self.path_ok { "formslot".into() } else { format!("\"{}\"", self.bin.display()) }
    }

    fn path_env(&self) -> Option<OsString> {
        if self.path_ok { self.path_dir.as_deref().and_then(engine::path_with) } else { None }
    }

    fn step_check(&mut self) -> bool {
        self.ui.heading(1, "Check this computer", "formslot needs git to read your projects; the other tools make it more useful.");
        let mut git_ok = true;
        for t in engine::check_tools() {
            match &t.found {
                Some(found) => self.ui.tag("found", &format!("{}: {}", t.name, found)),
                None => {
                    let label = match t.need {
                        Need::Required => "required",
                        Need::Recommended => "recommended",
                        Need::Optional => "optional",
                    };
                    self.ui.tag("missing", &format!("{} ({label}): {}.", t.name, t.purpose));
                    if !t.hint.is_empty() {
                        self.ui.say(&format!("          To get it, run: {}", t.hint));
                    }
                    if t.need == Need::Required {
                        git_ok = false;
                    } else {
                        self.skipped.push(format!("{} is not installed (setup never installs programs for you)", t.name));
                    }
                }
            }
        }
        if !git_ok {
            self.ui.say("");
            self.ui.say("git is needed before formslot can go on. Install it with the command above, open a new terminal, and run `formslot setup` again.");
        }
        git_ok
    }

    fn step_install(&mut self) {
        self.ui.heading(2, "Put formslot on this computer", "so you can type `formslot` in any terminal window, and Claude Code can find it.");
        let Some(dir) = engine::user_bin_dir() else {
            self.ui.tag("skipped", "could not work out a folder for your programs");
            self.skipped.push("copying formslot to a program folder".into());
            return;
        };
        self.path_dir = Some(dir.clone());
        let dry = self.opts.dry;
        let exe = self.bin.clone();
        let target = dir.join(engine::binary_name());
        match engine::placement(&exe, &dir) {
            Placement::Same => {
                self.ui.tag("done", &format!("formslot is already in {}", dir.display()));
                self.kept.push("formslot is in your program folder".into());
                self.bin = target;
            }
            state => {
                let (plain, ask) = if state == Placement::Absent {
                    (format!("copy formslot to {}", dir.display()), format!("Copy formslot to {}?", dir.display()))
                } else {
                    (format!("replace the older copy of formslot in {}", dir.display()), format!("Replace the older copy of formslot in {}?", dir.display()))
                };
                if dry {
                    self.ui.tag("would", &plain);
                } else if self.ui.confirm(&ask, true) {
                    match critical(|| engine::place_binary(&exe, &dir)) {
                        Ok(dest) => {
                            self.ui.tag("done", &format!("copied to {}", dest.display()));
                            self.done.push(format!("copied formslot to {}", dest.display()));
                            self.bin = dest;
                        }
                        Err(e) => {
                            self.ui.tag("problem", &e);
                            self.skipped.push("copying formslot to a program folder (it failed)".into());
                            return;
                        }
                    }
                } else {
                    self.ui.tag("skipped", "left as it is");
                    self.skipped.push("copying formslot to a program folder".into());
                    return;
                }
            }
        }
        self.step_path(&dir);
    }

    fn step_path(&mut self, dir: &Path) {
        if engine::dir_on_path(dir) {
            self.ui.tag("done", &format!("{} is already where your terminal looks for programs", dir.display()));
            self.path_ok = true;
            return;
        }
        if cfg!(windows) {
            self.step_path_windows(dir);
            return;
        }
        let plan = engine::rc_plan(dir);
        if engine::rc_has(&plan) {
            self.ui.tag("done", &format!("{} already adds it; open a new terminal window to use it", plan.file.as_ref().map(|f| f.display().to_string()).unwrap_or_default()));
            self.kept.push("your shell settings already include the formslot folder".into());
            self.path_ok = true;
            return;
        }
        self.ui.say(&format!("Your terminal does not look in {} yet. This line fixes that:", dir.display()));
        self.ui.say(&format!("    {}", plan.line));
        let Some(file) = plan.file.clone() else {
            self.ui.tag("skipped", "I do not know your shell's settings file; add the line above to it yourself, then open a new terminal");
            self.skipped.push("adding the formslot folder to your PATH (add the line yourself)".into());
            return;
        };
        if self.opts.no_path {
            self.ui.tag("skipped", &format!("--no-path: {} was not touched", file.display()));
            self.skipped.push(format!("adding the formslot folder to {}", file.display()));
            return;
        }
        if self.opts.dry {
            self.ui.tag("would", &format!("save a backup of {} and add that line to it", file.display()));
            return;
        }
        if !self.ui.confirm(&format!("Add it to {} now? A backup copy is saved first.", file.display()), true) {
            self.ui.tag("skipped", "not changed; add the line yourself when you like");
            self.skipped.push(format!("adding the formslot folder to {}", file.display()));
            return;
        }
        match critical(|| engine::add_to_rc(&plan)) {
            Ok(change) => {
                if let Some(b) = change.backup {
                    self.ui.say(&format!("Backup saved as {}", b.display()));
                }
                self.ui.tag("done", &format!("added to {}. Open a new terminal window for it to take effect.", file.display()));
                self.done.push(format!("added the formslot folder to {}", file.display()));
                self.path_ok = true;
            }
            Err(e) => {
                self.ui.tag("problem", &e);
                self.skipped.push("adding the formslot folder to your PATH (it failed)".into());
            }
        }
    }

    fn step_path_windows(&mut self, dir: &Path) {
        let script = engine::windows_path_script(dir);
        if self.opts.no_path {
            self.ui.tag("skipped", "--no-path: your PATH was not touched");
            self.ui.say(&format!("To add it yourself, run in PowerShell: {script}"));
            self.skipped.push("adding the formslot folder to your PATH".into());
            return;
        }
        if self.opts.dry {
            self.ui.tag("would", &format!("add {} to your user PATH", dir.display()));
            return;
        }
        if !self.ui.confirm(&format!("Add {} to your PATH so new terminals find formslot?", dir.display()), true) {
            self.ui.tag("skipped", "not changed");
            self.ui.say(&format!("To add it yourself, run in PowerShell: {script}"));
            self.skipped.push("adding the formslot folder to your PATH".into());
            return;
        }
        let ok = critical(|| Command::new("powershell").args(["-NoProfile", "-Command", &script]).status().map(|s| s.success()).unwrap_or(false));
        if ok {
            self.ui.tag("done", "added to your user PATH. Open a new terminal window for it to take effect.");
            self.done.push("added the formslot folder to your PATH".into());
            self.path_ok = true;
        } else {
            self.ui.tag("problem", "PowerShell did not accept it");
            self.ui.say(&format!("Run this in PowerShell yourself: {script}"));
            self.skipped.push("adding the formslot folder to your PATH (it failed)".into());
        }
    }

    fn step_service(&mut self) {
        self.ui.heading(3, "Start formslot", "a small helper that runs quietly on this computer only and answers Claude Code. It stops by itself when idle.");
        let (running, line) = if config::data_dir_path().exists() { run_self(&self.bin, &["status"], None, false) } else { (false, String::new()) };
        if running {
            self.ui.tag("done", &format!("already running: {}", first_line(&line)));
            self.kept.push("the service was already running".into());
            return;
        }
        if self.opts.dry {
            self.ui.tag("would", "start the service and check it answers");
            return;
        }
        let (started, text) = run_self(&self.bin, &["start"], None, false);
        if !started {
            self.ui.tag("problem", &first_line(&text));
            self.ui.say("You can still go on; run `formslot doctor` afterwards to see what is wrong.");
            self.skipped.push("starting the service (it failed)".into());
            return;
        }
        let (_, health) = run_self(&self.bin, &["status"], None, false);
        self.ui.tag("done", &first_line(&text));
        self.ui.say(&format!("Healthy: {}", first_line(&health)));
        self.done.push("started the service".into());
    }

    fn validate(&self, raw: &str) -> Result<(PathBuf, String), String> {
        let path = resolve(raw);
        if !path.is_dir() {
            return Err(format!("I cannot find a folder at {}. Check the spelling, or drag the folder from your file manager into this window.", path.display()));
        }
        let repo = Repo::open(&path).map_err(|_| format!("{} is a folder, but not a git repository (git has not been set up in it). formslot works on git projects; pick another folder, or run `git init` there first.", path.display()))?;
        if repo.rev_parse("HEAD").is_err() {
            return Err(format!("{} is a git repository with no saved commits yet. Make a first commit, then add it.", path.display()));
        }
        let top = repo.toplevel().to_path_buf();
        let name = top.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        Ok((top, name))
    }

    fn index(&mut self, raw: &str) -> Result<String, String> {
        let (top, name) = self.validate(raw)?;
        let known = indexed().iter().any(|(_, p, _)| Path::new(p) == top);
        if self.opts.dry {
            self.ui.tag("would", &format!("{} {} ({})", if known { "refresh" } else { "index" }, name, top.display()));
            return Ok(name);
        }
        self.ui.say(&format!("Reading {name}. A first read of a big project can take a minute."));
        let stop = Arc::new(AtomicBool::new(false));
        let spin = io::stdout().is_terminal().then(|| {
            let stop = stop.clone();
            let label = name.clone();
            thread::spawn(move || {
                let began = Instant::now();
                while !stop.load(Ordering::SeqCst) {
                    print!("\r  ... reading {label}, {}s ", began.elapsed().as_secs());
                    let _ = io::stdout().flush();
                    thread::sleep(Duration::from_millis(250));
                }
                print!("\r\x1b[K");
                let _ = io::stdout().flush();
            })
        });
        let result = Store::open(&config::corpus_db()).and_then(|mut store| corpus::add(&mut store, &top, "HEAD"));
        stop.store(true, Ordering::SeqCst);
        if let Some(handle) = spin {
            let _ = handle.join();
        }
        let (found, run) = result.map_err(|e| format!("indexing {name} failed: {e}"))?;
        self.ui.tag("done", &format!("{}: {} files read in {} ms{}", found.name, found.files, run.millis, if known { " (refreshed)" } else { "" }));
        if run.secrets > 0 {
            self.ui.say(&format!("Note: {} possible passwords or keys were found in {}. formslot keeps them out of what it shows to Claude.", run.secrets, found.name));
        }
        self.done.push(format!("{} {}", if known { "refreshed" } else { "indexed" }, found.name));
        self.added.push(top.to_string_lossy().to_string());
        Ok(found.name)
    }

    fn step_projects(&mut self) {
        self.ui.heading(4, "Add your projects", "formslot can only help with projects it has read. Each one must be a git repository.");
        let repos = std::mem::take(&mut self.opts.repos);
        for r in &repos {
            if let Err(e) = self.index(r) {
                self.ui.tag("problem", &e);
                self.skipped.push(format!("adding {r} (it failed)"));
                self.failed = true;
            }
        }
        let existing = indexed();
        if self.opts.yes || self.opts.dry {
            if repos.is_empty() {
                self.ui.tag("skipped", "no project given; add one later with `formslot add PATH` or run `formslot setup --repo PATH`");
                self.skipped.push("adding a project".into());
            }
            self.list();
            return;
        }
        if !existing.is_empty() {
            self.list();
            if !self.ui.confirm("Add another project?", false) {
                return;
            }
        }
        let cwd = std::env::current_dir().ok().and_then(|d| Repo::open(&d).ok()).map(|r| r.toplevel().to_path_buf());
        let mut default = cwd.filter(|c| !indexed().iter().any(|(_, p, _)| Path::new(p) == c.as_path())).map(|c| c.display().to_string());
        let mut count = 0;
        loop {
            let question = if default.is_some() { "Which folder holds your project? (you can drag it into this window)" } else { "Folder of a project to add (drag it here), or press Enter to skip:" };
            let answer = self.ui.ask(question, default.as_deref());
            if answer.trim().is_empty() {
                break;
            }
            default = None;
            match self.index(&answer) {
                Ok(_) => {
                    count += 1;
                    if !self.ui.confirm("Add another project?", false) {
                        break;
                    }
                }
                Err(e) => self.ui.say(&format!("{e}\n  Try another folder, or press Enter to skip.")),
            }
        }
        if count == 0 && existing.is_empty() {
            self.skipped.push("adding a project (add one later with `formslot add PATH`)".into());
        }
        self.list();
    }

    fn list(&self) {
        let all = indexed();
        if all.is_empty() {
            return;
        }
        self.ui.say(&format!("Projects formslot has read ({}):", all.len()));
        for (name, path, files) in all {
            self.ui.say(&format!("  {name}  {files} files  {path}"));
        }
    }

    fn step_connect(&mut self) {
        self.ui.heading(5, "Connect Claude Code", "this is what lets Claude Code ask formslot before it writes code that already exists.");
        let candidates: Vec<(String, String)> = if self.opts.yes || self.opts.dry {
            indexed().into_iter().filter(|(_, p, _)| self.added.contains(p)).map(|(n, p, _)| (n, p)).collect()
        } else {
            indexed().into_iter().map(|(n, p, _)| (n, p)).collect()
        };
        if self.opts.dry {
            self.ui.tag("would", "add the formslot settings to each project's .claude/settings.json and .mcp.json (other settings kept), then tell you to approve the server once in Claude Code");
            return;
        }
        if candidates.is_empty() {
            self.ui.tag("skipped", "no project to connect yet");
            self.skipped.push("connecting Claude Code (no project yet)".into());
            return;
        }
        let mut todo: Vec<(String, String)> = Vec::new();
        let mut linked: Vec<(String, String)> = Vec::new();
        for (name, path) in candidates {
            if !Path::new(&path).is_dir() {
                self.ui.tag("skipped", &format!("{name}: the folder {path} is gone"));
            } else if is_connected(&self.bin, &path, self.path_env().as_ref()) {
                self.ui.tag("done", &format!("{name} is already connected"));
                self.kept.push(format!("{name} was already connected to Claude Code"));
                linked.push((name, path));
            } else {
                todo.push((name, path));
            }
        }
        if !todo.is_empty() {
            self.ui.say("Connecting adds a few settings to the project's .claude/settings.json and a formslot entry to its .mcp.json. Your other settings are kept exactly as they are, and formslot starts in a gentle mode: it points out code that already exists or ideas you ruled out, but it never blocks Claude from working.");
            let names: Vec<&str> = todo.iter().map(|(n, _)| n.as_str()).collect();
            if self.opts.dry {
                self.ui.tag("would", &format!("connect {}", names.join(", ")));
            } else if self.ui.confirm(&format!("Connect {} now?", names.join(", ")), true) {
                let path_env = self.path_env();
                for (name, path) in todo {
                    let (ok, text) = critical(|| run_self(&self.bin, &["install", &path, "--mode", "warn"], path_env.as_ref(), true));
                    if ok {
                        self.ui.tag("done", &format!("{name} is connected"));
                        self.done.push(format!("connected {name} to Claude Code"));
                        linked.push((name, path));
                    } else {
                        self.ui.tag("problem", &format!("{name}: {}", first_line(&text)));
                        self.skipped.push(format!("connecting {name} (it failed)"));
                        self.failed = true;
                    }
                }
            } else {
                self.ui.tag("skipped", "not connected; run `formslot install FOLDER` when you want to");
                self.skipped.push("connecting Claude Code".into());
            }
        }
        if !linked.is_empty() {
            self.ui.say("");
            self.ui.say("One step is left that only you can do. Claude Code asks before it trusts a new tool, so, for each project:");
            for (name, path) in &linked {
                self.ui.say(&format!("  {name}:  cd \"{path}\"   then   claude"));
            }
            self.ui.say("When Claude Code asks whether to use the \"formslot\" server, choose to approve it. You only do this once per project.");
        }
    }

    fn step_finish(&mut self) {
        self.ui.heading(6, "Final check", "a last look that everything is in place.");
        if self.opts.dry {
            self.ui.tag("would", "run `formslot doctor`");
        } else {
            let path_env = self.path_env();
            let (_, text) = run_self(&self.bin, &["doctor"], path_env.as_ref(), false);
            let mut shown = 0;
            for line in text.lines() {
                if line.contains("PROBLEM") || line.contains(": warning") || line.starts_with("doctor:") {
                    self.ui.say(line);
                    shown += 1;
                }
            }
            if shown == 0 {
                self.ui.say("The check gave no answer; run `formslot doctor` yourself.");
            }
        }
        println!();
        println!("{}", self.ui.paint("1", "Summary"));
        let block = |label: &str, items: &[String]| {
            if !items.is_empty() {
                println!("  {label}");
                for i in items {
                    println!("    - {i}");
                }
            }
        };
        if self.opts.dry {
            println!("  Dry run: nothing was changed.");
        }
        block("Done now:", &self.done);
        block("Already in place:", &self.kept);
        block("Skipped:", &self.skipped);
        let first = quoted(&indexed().into_iter().next().map(|(n, _, _)| n).unwrap_or_else(|| "NAME".into()));
        let w = self.word();
        println!();
        println!("{}", self.ui.paint("1", "Try these next"));
        println!("  {w} search \"login form\" --corpus {first}      find code and rules by describing them");
        println!("  {w} pack \"add a settings page\" --corpus {first}   what Claude Code would be told for a task");
        println!("  {w} status                                  see that formslot is running");
        if !self.path_ok {
            println!("  (`formslot` alone will work once it is on your PATH; until then run it by its full path, as above.)");
        } else if !engine::dir_on_path(self.path_dir.as_deref().unwrap_or(Path::new(""))) {
            println!("  (Open a new terminal window first so the `formslot` command is found.)");
        }
        println!();
        println!("Your data lives in {}", config::data_dir_path().display());
        println!("Everything else: formslot help");
    }
}

pub fn bare() -> ExitCode {
    arm_interrupt();
    let ui = Ui::new(false);
    let projects = indexed();
    if projects.is_empty() {
        println!("{}", ui.paint("1", &format!("Welcome to formslot {VERSION}")));
        println!("formslot remembers what your projects already contain, such as functions, components and the rules you set, and hands it to Claude Code so it reuses your code instead of writing it again.");
        println!("It runs on this computer, and reading your projects does not send them anywhere.");
        println!();
        if ui.confirm("Set it up now?", true) {
            return run(&[]);
        }
        println!("No problem. Run `formslot setup` when you are ready, or `formslot help` to see every command.");
        return ExitCode::SUCCESS;
    }
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("formslot"));
    let (_, line) = run_self(&exe, &["status"], None, false);
    let line = first_line(&line);
    println!("{}", ui.paint("1", &format!("formslot {VERSION}")));
    println!("  Service:  {}", if line.is_empty() { "not running (start it with `formslot start`)".to_string() } else { line });
    let names: Vec<&str> = projects.iter().map(|(n, _, _)| n.as_str()).collect();
    println!("  Projects: {} indexed ({})", projects.len(), names.join(", "));
    println!();
    let first = quoted(&projects[0].0);
    println!("Useful commands:");
    println!("  formslot search \"words\" --corpus {first}   find code and rules by describing them");
    println!("  formslot pack \"a task\" --corpus {first}    what Claude Code would be told for a task");
    println!("  formslot add FOLDER                    read another project");
    println!("  formslot doctor                        check that everything is in place");
    println!("  formslot setup                         run the guided setup again");
    println!();
    println!("formslot help for everything.");
    ExitCode::SUCCESS
}

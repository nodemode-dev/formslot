use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use formslot_core::discovery::{self, ServiceInfo};
use formslot_core::{CONTRACT, VERSION, api, binres, config, http, install, lifecycle, log};
use serde_json::{Value, json};

const START_WAIT: Duration = Duration::from_secs(20);
const STOP_WAIT: Duration = Duration::from_secs(15);

fn call(info: &ServiceInfo, method: &str, path: &str, body: Option<&str>) -> Result<(u16, Value), String> {
    let addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, info.port);
    let mut stream = TcpStream::connect_timeout(&addr.into(), Duration::from_secs(2)).map_err(|e| e.to_string())?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let body = body.unwrap_or("");
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        info.port,
        info.token,
        body.len()
    );
    stream.write_all(request.as_bytes()).map_err(|e| e.to_string())?;
    let mut raw = String::new();
    stream.read_to_string(&mut raw).map_err(|e| e.to_string())?;
    let status: u16 = raw.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let text = raw.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or("");
    Ok((status, serde_json::from_str(text).unwrap_or(Value::Null)))
}

fn health(info: &ServiceInfo) -> Result<Value, String> {
    match call(info, "GET", "/api/health", None)? {
        (200, value) if value.is_object() => Ok(value),
        (200, _) => Err("the health answer was not JSON".to_string()),
        (status, _) => Err(format!("the service answered {status}")),
    }
}

fn live() -> Option<ServiceInfo> {
    let info = discovery::read().ok().flatten()?;
    (discovery::pid_alive(info.pid) && health(&info).is_ok()).then_some(info)
}

pub fn serve(args: &[String]) -> ExitCode {
    if let Some(other) = args.iter().find(|a| *a != "--detach") {
        eprintln!("formslot: serve takes only --detach (got {other})");
        return ExitCode::from(2);
    }
    if args.iter().any(|a| a == "--detach") {
        return start();
    }
    let lock = match discovery::acquire_lock() {
        Ok(Some(lock)) => lock,
        Ok(None) => {
            return match discovery::read() {
                Ok(Some(info)) => {
                    println!("formslot is already running on 127.0.0.1:{} (pid {})", info.port, info.pid);
                    ExitCode::SUCCESS
                }
                _ => {
                    println!("formslot is already running (its discovery file is not written yet)");
                    ExitCode::SUCCESS
                }
            };
        }
        Err(e) => {
            eprintln!("formslot: {e}");
            return ExitCode::FAILURE;
        }
    };
    if let Ok(Some(old)) = discovery::read()
        && !discovery::pid_alive(old.pid)
    {
        discovery::remove();
    }
    if let Err(e) = ctrlc::set_handler(lifecycle::request_stop) {
        eprintln!("formslot: signal handlers could not be installed ({e})");
        return ExitCode::FAILURE;
    }
    let token = discovery::new_token();
    let server = match http::start(config::port(), token.clone(), api::route) {
        Ok(server) => server,
        Err(e) => {
            eprintln!("formslot: {e}");
            return ExitCode::FAILURE;
        }
    };
    let info = ServiceInfo::new(server.port, token);
    if let Err(e) = discovery::write(&info) {
        eprintln!("formslot: {e}");
        server.stop();
        return ExitCode::FAILURE;
    }
    let idle = lifecycle::idle_limit().map(|d| d.as_secs());
    log::record("serve", json!({ "port": server.port, "version": VERSION, "idle_limit_seconds": idle }));
    println!("formslot serves on http://127.0.0.1:{}", server.port);
    let reason = loop {
        if let Some(reason) = lifecycle::exit_reason() {
            break reason;
        }
        thread::sleep(Duration::from_millis(200));
    };
    server.stop();
    discovery::remove();
    log::record("serve-stop", json!({ "reason": reason }));
    drop(lock);
    ExitCode::SUCCESS
}

pub fn start() -> ExitCode {
    if let Some(info) = live() {
        println!("formslot is already running on 127.0.0.1:{} (pid {})", info.port, info.pid);
        return ExitCode::SUCCESS;
    }
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(e) => {
            eprintln!("formslot: cannot find this program to start it ({e})");
            return ExitCode::FAILURE;
        }
    };
    let mut cmd = Command::new(exe);
    cmd.arg("serve").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0000_0008 | 0x0000_0200 | 0x0800_0000);
    }
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            eprintln!("formslot: the service could not be started ({e})");
            return ExitCode::FAILURE;
        }
    };
    let began = Instant::now();
    loop {
        if let Some(info) = live() {
            println!("formslot started on 127.0.0.1:{} (pid {})", info.port, info.pid);
            return ExitCode::SUCCESS;
        }
        match child.try_wait() {
            Ok(Some(status)) if !status.success() => {
                eprintln!("formslot: the service exited at start ({status}); run `formslot serve` in a terminal to see why");
                return ExitCode::FAILURE;
            }
            Ok(Some(_)) if began.elapsed() > Duration::from_secs(2) => {
                eprintln!("formslot: the service did not stay up; run `formslot serve` in a terminal to see why");
                return ExitCode::FAILURE;
            }
            _ => {}
        }
        if began.elapsed() > START_WAIT {
            eprintln!("formslot: the service did not answer within {} seconds; run `formslot doctor`", START_WAIT.as_secs());
            return ExitCode::FAILURE;
        }
        thread::sleep(Duration::from_millis(100));
    }
}

pub fn stop() -> ExitCode {
    let info = match discovery::read() {
        Ok(Some(info)) => info,
        Ok(None) => {
            println!("formslot is not running");
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("formslot: {e}");
            return ExitCode::FAILURE;
        }
    };
    if !discovery::pid_alive(info.pid) {
        discovery::remove();
        println!("formslot is not running (removed the stale discovery file of pid {})", info.pid);
        return ExitCode::SUCCESS;
    }
    if let Err(e) = call(&info, "POST", "/api/shutdown", Some("{}")) {
        eprintln!("formslot: the service did not take the stop request ({e})");
        return ExitCode::FAILURE;
    }
    let began = Instant::now();
    while began.elapsed() < STOP_WAIT {
        if !discovery::pid_alive(info.pid) {
            println!("formslot stopped (pid {})", info.pid);
            return ExitCode::SUCCESS;
        }
        thread::sleep(Duration::from_millis(100));
    }
    eprintln!("formslot: pid {} is still running after {} seconds", info.pid, STOP_WAIT.as_secs());
    ExitCode::FAILURE
}

pub fn status() -> ExitCode {
    let info = match discovery::read() {
        Ok(Some(info)) => info,
        Ok(None) => {
            println!("formslot is not running");
            return ExitCode::from(1);
        }
        Err(e) => {
            eprintln!("formslot: {e}");
            return ExitCode::FAILURE;
        }
    };
    if !discovery::pid_alive(info.pid) {
        println!("formslot is not running (the discovery file is stale: pid {} is gone)", info.pid);
        return ExitCode::from(1);
    }
    match health(&info) {
        Ok(answer) => {
            let version = answer.get("version").and_then(Value::as_str).unwrap_or("?");
            let contract = answer.get("contract").and_then(Value::as_u64).unwrap_or(0);
            println!(
                "formslot {version} is running on 127.0.0.1:{} (pid {}, contract {contract}, since {})",
                info.port, info.pid, info.started
            );
            if contract != u64::from(CONTRACT) {
                println!("note: this command line speaks contract {CONTRACT}");
            }
            if let Ok((200, lease)) = call(&info, "GET", "/api/lease", None) {
                let names: Vec<&str> = lease
                    .get("leases")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(|l| l.get("client").and_then(Value::as_str)).collect())
                    .unwrap_or_default();
                let idle = lease.get("idle_seconds").and_then(Value::as_u64).unwrap_or(0);
                match lease.get("idle_limit_seconds").and_then(Value::as_u64) {
                    Some(limit) => println!("leases: {} {}; idle {idle}s, stops at {limit}s with no lease and no request", names.len(), names.join(", ")),
                    None => println!("leases: {} {}; idle shutdown is off", names.len(), names.join(", ")),
                }
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            println!("formslot is not answering on 127.0.0.1:{} (pid {}): {e}", info.port, info.pid);
            ExitCode::from(1)
        }
    }
}

struct Report {
    problems: usize,
    warnings: usize,
}

impl Report {
    fn line(&self, name: &str, text: &str) {
        println!("{name}: {text}");
    }

    fn problem(&mut self, name: &str, text: &str) {
        self.problems += 1;
        println!("{name}: PROBLEM {text}");
    }

    fn warn(&mut self, name: &str, text: &str) {
        self.warnings += 1;
        println!("{name}: warning {text}");
    }
}

#[cfg(unix)]
fn mode_of(path: &Path) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).ok().map(|m| m.permissions().mode() & 0o777)
}

fn modes(report: &mut Report, dir: &Path) {
    #[cfg(unix)]
    {
        let expect: [(PathBuf, u32); 6] = [
            (dir.to_path_buf(), 0o700),
            (dir.join("service.json"), 0o600),
            (dir.join("service.lock"), 0o600),
            (dir.join("corpus.sqlite"), 0o600),
            (dir.join("formslot.log"), 0o600),
            (dir.join("dense"), 0o700),
        ];
        let mut seen = Vec::new();
        for (path, want) in expect {
            let Some(mode) = mode_of(&path) else { continue };
            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            seen.push(format!("{name} {mode:o}"));
            if mode != want {
                report.warn("modes", &format!("{} is {mode:o}, expected {want:o}", path.display()));
            }
        }
        report.line("modes", &seen.join(", "));
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
        report.line("modes", "not checked on this system");
    }
}

fn store_version(report: &mut Report) {
    let path = config::corpus_db();
    if !path.exists() {
        report.line("store", "none yet (created on first use)");
        return;
    }
    let opened = rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .and_then(|c| c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0)));
    match opened {
        Ok(version) => report.line("store", &format!("{} user_version {version}", path.display())),
        Err(e) => report.problem("store", &format!("{} could not be read ({e})", path.display())),
    }
}

fn plugin_version() -> Option<(PathBuf, String)> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(dir) = std::env::var_os("FORMSLOT_PLUGIN_DIR").filter(|v| !v.is_empty()) {
        candidates.push(PathBuf::from(dir).join(".claude-plugin").join("plugin.json"));
    }
    if let Ok(exe) = std::env::current_exe() {
        for dir in exe.ancestors().skip(1).take(6) {
            candidates.push(dir.join("plugin").join("formslot").join(".claude-plugin").join("plugin.json"));
        }
    }
    candidates.into_iter().find_map(|path| {
        let text = std::fs::read_to_string(&path).ok()?;
        let value: Value = serde_json::from_str(&text).ok()?;
        Some((path, value.get("version")?.as_str()?.to_string()))
    })
}

fn resolves(program: &str) -> bool {
    if program.contains('/') || program.contains('\\') {
        Path::new(program).is_file()
    } else {
        binres::on_path(program).is_some()
    }
}

fn repo_commands(report: &mut Report, repo: &Path) {
    let mut commands: Vec<(String, String)> = Vec::new();
    if let Some(value) = std::fs::read_to_string(repo.join(".claude").join("settings.json")).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) {
        if let Some(events) = value.get("hooks").and_then(Value::as_object) {
            for (event, groups) in events {
                for group in groups.as_array().into_iter().flatten() {
                    for hook in group.get("hooks").and_then(Value::as_array).into_iter().flatten() {
                        if let Some(c) = hook.get("command").and_then(Value::as_str).filter(|c| install::is_ours(c)) {
                            commands.push((format!("hooks.{event}"), install::program_of(c)));
                        }
                    }
                }
            }
        }
    }
    if let Some(value) = std::fs::read_to_string(repo.join(".mcp.json")).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok())
        && let Some(c) = value.pointer("/mcpServers/formslot/command").and_then(Value::as_str)
    {
        commands.push(("mcpServers.formslot".to_string(), c.to_string()));
    }
    if commands.is_empty() {
        report.line("repo hooks", &format!("{} has no formslot hooks or server entry", repo.display()));
        return;
    }
    for (place, program) in commands {
        if resolves(&program) {
            report.line("repo hooks", &format!("{place}: {program} is found"));
        } else {
            report.problem("repo hooks", &format!("{place}: {program} is not found; run `formslot install {}` again", repo.display()));
        }
    }
}

pub fn doctor(args: &[String]) -> ExitCode {
    let mut report = Report { problems: 0, warnings: 0 };
    let repo = match args {
        [] => std::env::current_dir().ok().filter(|d| d.join(".claude").join("settings.json").is_file() || d.join(".mcp.json").is_file()),
        [one] if !one.starts_with("--") => Some(PathBuf::from(one)),
        _ => {
            eprintln!("formslot: usage: formslot doctor [<repo>]");
            return ExitCode::from(2);
        }
    };
    println!("formslot {VERSION} (contract {CONTRACT})");
    let dir = config::data_dir();
    let probe = dir.join(".doctor-probe");
    match std::fs::write(&probe, b"") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            report.line("data folder", &format!("{} (writable)", dir.display()));
        }
        Err(e) => report.problem("data folder", &format!("{} is not writable ({e})", dir.display())),
    }
    modes(&mut report, &dir);
    report.line("port", &config::port().to_string());
    match Command::new("git").arg("--version").output() {
        Ok(out) if out.status.success() => report.line("git", String::from_utf8_lossy(&out.stdout).trim()),
        _ => report.problem("git", "not found on PATH (formslot needs it to index repositories)"),
    }
    match discovery::read() {
        Ok(Some(info)) if discovery::pid_alive(info.pid) => match health(&info) {
            Ok(_) => report.line("service", &format!("running on 127.0.0.1:{} (pid {}), started {}", info.port, info.pid, info.started)),
            Err(e) => report.problem("service", &format!("pid {} is alive but does not answer on port {} ({e})", info.pid, info.port)),
        },
        Ok(Some(info)) => report.warn("service", &format!("not running; the discovery file is stale (pid {} is gone) and the next start removes it", info.pid)),
        Ok(None) => report.line("service", "not running (it starts with `formslot start`)"),
        Err(e) => report.problem("service", &e),
    }
    match lifecycle::idle_limit() {
        Some(limit) => report.line("idle shutdown", &format!("{} minutes with no lease and no request", limit.as_secs() / 60)),
        None => report.line("idle shutdown", "off"),
    }
    store_version(&mut report);
    for name in ["gitleaks", "rg", "curl"] {
        match binres::find(name) {
            Some(path) => report.line(name, &format!("{} ({})", path.display(), binres::source(name))),
            None => report.line(name, &format!("not found (optional; set {} or put it in {})", binres::env_name(name), dir.join("bin").display())),
        }
    }
    let exe = std::env::current_exe().ok().and_then(|p| p.canonicalize().ok());
    match binres::on_path("formslot") {
        Some(path) => report.line("hooks", &format!("`formslot` is on PATH at {}; install writes `formslot`", path.display())),
        None => {
            report.line("hooks", &format!("`formslot` is not on PATH; install writes {}", install::launcher_path()));
            if let Some(exe) = &exe {
                report.line("this binary", &exe.display().to_string());
            }
        }
    }
    if let Some(repo) = repo {
        repo_commands(&mut report, &repo);
    }
    report.line("dense feature", if cfg!(feature = "dense") { "built in" } else { "not built (cargo build --features dense)" });
    match plugin_version() {
        Some((path, version)) => {
            let note = if version == VERSION { String::new() } else { format!(" (this binary is {VERSION})") };
            report.line("plugin", &format!("{version} at {}{note}", path.display()));
        }
        None => report.line("plugin", "not found near this binary (fine for a standalone install)"),
    }
    println!("doctor: {} problems, {} warnings", report.problems, report.warnings);
    if report.problems == 0 { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

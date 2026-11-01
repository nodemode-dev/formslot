use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpStream};
use std::process::{Command, ExitCode};
use std::time::Duration;

mod corpus_cmd;
mod dedup_cmd;
mod dense_cmd;
mod eval_cmd;
mod hook_cmd;
mod mcp_cmd;
mod memory_cmd;
mod search_cmd;
mod slot_cmd;
mod symbols_cmd;

use formslot_core::discovery::{self, ServiceInfo};
use formslot_core::{CONTRACT, VERSION, api, config, http, log};
use serde_json::{Value, json};

const HELP: &str = "formslot: the Form family's context service

  serve      run the service on 127.0.0.1 until stopped (one per data folder)
  status     say whether the service is running, and its health
  doctor     print the data folder, git, and the discovery file
  add <path> [--ref <rev>]          index a repository at a commit (HEAD)
  reindex <name> [--ref <rev>]      index again; only blobs not seen before are read
  stats <name> [--json]             files, lines and bytes by language, roles, licences
  files <name> [--role R] [--language L]   list indexed paths
  family --family-json <path> [--check]    the family list from formMCP's family.json
  symbols <name> [--lang L] [--kind K] [--exported] [--count]   extracted units
  show <name> <path>#<qualified>            one unit, what it calls, who uses it
  refs <name> <symbol>                      every edge that points at a name
  dupes <name> | dupes --across <a> <b>     clone groups and near-duplicates by tier
  drift <a> <b>                             how shared code differs between two corpora
  slot list [--status S]            the slots in the store
  slot get <id> [--json]            one slot: files, copies, events
  slot seed <dir>                   import a folder of manifests as proposed
  slot refresh <id|--all> --corpus <name>   hashes, lines, contentHash and copies from a corpus
  slot propose <file.json>          store a proposal
  slot confirm <id> --owner         the owner makes a slot canonical
  slot decline <id> --owner --reason <text>   the owner declines a slot
  slot plan --repo <path> <id>... [--force]  files, targets and order for a consumer
  slot apply --repo <path> <id>... [--dry-run] [--force]   write slots.lock.json only
  slot drift --repo <path> [--infer] [--check] [--json]   clean, local, behind, diverged, missing
  memory ingest <corpus>            read rules, declines, HASHOUT entries and decisions into memory records
  memory list [--kind K] [--status S] [--corpus C] [--limit N] [--count] [--json]   the records
  memory declines [--check <corpus>] [--json]   the decline specs; --check fails (exit 1) when one no longer finds its owner line
  recall <query> [--anchor path#symbol] [--kind K] [--corpus C] [--limit N] [--json]   ranked records, with a token estimate
  remember --propose <text> [--anchor path#symbol] [--kind decision|how-to]   store a proposal and print NOOP, MERGE, SUPERSEDE or ADD
  search <query> [--corpus C] [--limit N] [--json]   ranked units, slots and memory records
  search rebuild [--corpus C]       rebuild the search indexes
  search <query> --dense            add the dense list to the fusion (build with --features dense)
  dense fetch [--yes]               show, then download, the gte-modernbert-base model (dense builds only)
  dense build [--corpus C] [--rebuild]   embed units into the vec0 table (dense builds only)
  dense status                      the model, sqlite-vec and the embedded units per corpus
  map <corpus> [--tokens N] [--formmap]   the ranked repo map, or FORMMAP.md under 150 lines
  pack <task> [--corpus C] [--tokens N] [--tier T1|T4|T16] [--json]   a P0/P1/P2 context pack
  eval [--gold F] [--corpus C] [--arm formslot|baseline|dense|all] [--k 1,4,10] [--json] [--out F]   recall, MRR, nDCG, tokens and latency over a gold set
  mcp        serve MCP over stdio (newline-delimited JSON-RPC) for an agent's .mcp.json
  check [--mode warn|deny|off]      PreToolUse hook: reads the hook JSON on stdin, answers declines and reuse
  hook <event>                      Claude Code hook: SessionStart, UserPromptSubmit, PostToolUse, PreCompact, SubagentStart, Stop
  install <repo> [--mode warn|deny] [--enable] [--dry-run]   merge the hooks and the MCP entry into a repository, keeping every other key
  version    print the version
  help       this text

  FORMSLOT_PORT       the port (5645); 0 asks the system for a free one
  FORMSLOT_DATA_DIR   the data folder (~/.formslot)
";

fn quiet_on_closed_pipe() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let text = info
            .payload()
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| info.payload().downcast_ref::<&str>().copied())
            .unwrap_or("");
        if text.contains("Broken pipe") {
            std::process::exit(0);
        }
        default(info);
    }));
}

fn main() -> ExitCode {
    quiet_on_closed_pipe();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("help");
    match command {
        "serve" => serve(),
        "status" => status(),
        "doctor" => doctor(),
        "add" => corpus_cmd::add(&args[1..]),
        "reindex" => corpus_cmd::reindex(&args[1..]),
        "stats" => corpus_cmd::stats(&args[1..]),
        "files" => corpus_cmd::files(&args[1..]),
        "family" => corpus_cmd::family(&args[1..]),
        "slot" => slot_cmd::run(&args[1..]),
        "memory" => memory_cmd::run(&args[1..]),
        "recall" => memory_cmd::recall(&args[1..]),
        "remember" => memory_cmd::remember(&args[1..]),
        "symbols" => symbols_cmd::symbols(&args[1..]),
        "show" => symbols_cmd::show(&args[1..]),
        "refs" => symbols_cmd::refs(&args[1..]),
        "dupes" => dedup_cmd::dupes(&args[1..]),
        "drift" => dedup_cmd::drift(&args[1..]),
        "dense" => dense_cmd::run(&args[1..]),
        "search" => search_cmd::run(&args[1..]),
        "map" => search_cmd::map(&args[1..]),
        "pack" => search_cmd::pack(&args[1..]),
        "eval" => eval_cmd::run(&args[1..]),
        "check" => hook_cmd::check(&args[1..]),
        "hook" => hook_cmd::event(&args[1..]),
        "install" => hook_cmd::install(&args[1..]),
        "mcp" => mcp_cmd::run(&args[1..]),
        "version" | "--version" | "-V" => {
            println!("formslot {VERSION}");
            ExitCode::SUCCESS
        }
        "help" | "--help" | "-h" => {
            print!("{HELP}");
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("formslot: unknown command '{other}'\n");
            eprint!("{HELP}");
            ExitCode::from(2)
        }
    }
}

fn serve() -> ExitCode {
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
        return ExitCode::FAILURE;
    }
    log::record("serve", json!({ "port": server.port, "version": VERSION }));
    println!("formslot serves on http://127.0.0.1:{}", server.port);
    server.wait();
    discovery::remove();
    drop(lock);
    ExitCode::SUCCESS
}

fn health(info: &ServiceInfo) -> Result<Value, String> {
    let addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, info.port);
    let mut stream = TcpStream::connect_timeout(&addr.into(), Duration::from_secs(2)).map_err(|e| e.to_string())?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let request = format!(
        "GET /api/health HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {}\r\nConnection: close\r\n\r\n",
        info.port, info.token
    );
    stream.write_all(request.as_bytes()).map_err(|e| e.to_string())?;
    let mut raw = String::new();
    stream.read_to_string(&mut raw).map_err(|e| e.to_string())?;
    let status = raw.split_whitespace().nth(1).unwrap_or("0").to_string();
    if status != "200" {
        return Err(format!("the service answered {status}"));
    }
    let body = raw.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or("");
    serde_json::from_str(body).map_err(|e| format!("the health answer was not JSON ({e})"))
}

fn status() -> ExitCode {
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
            ExitCode::SUCCESS
        }
        Err(e) => {
            println!("formslot is not answering on 127.0.0.1:{} (pid {}): {e}", info.port, info.pid);
            ExitCode::from(1)
        }
    }
}

fn doctor() -> ExitCode {
    println!("formslot {VERSION} (contract {CONTRACT})");
    println!("data folder: {}", config::data_dir().display());
    println!("port: {}", config::port());
    match Command::new("git").arg("--version").output() {
        Ok(out) if out.status.success() => println!("git: {}", String::from_utf8_lossy(&out.stdout).trim()),
        _ => println!("git: not found on PATH (formslot needs it to index repositories)"),
    }
    match discovery::read() {
        Ok(Some(info)) => {
            let alive = if discovery::pid_alive(info.pid) { "alive" } else { "gone" };
            println!("discovery: port {}, pid {} ({alive}), started {}", info.port, info.pid, info.started);
        }
        Ok(None) => println!("discovery: none (the service is not running)"),
        Err(e) => println!("discovery: {e}"),
    }
    ExitCode::SUCCESS
}

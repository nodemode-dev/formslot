use std::process::ExitCode;

mod corpus_cmd;
mod dedup_cmd;
mod dense_cmd;
mod eval_cmd;
mod hook_cmd;
mod mcp_cmd;
mod memory_cmd;
mod search_cmd;
mod service_cmd;
mod slot_cmd;
mod symbols_cmd;

use formslot_core::VERSION;

const HELP: &str = "formslot: the Form family's context service

  serve [--detach]   run the service on 127.0.0.1 until stopped (one per data folder); --detach starts it in the background
  start      start the service in the background if none runs, wait for it, print its port and pid
  stop       stop the running service gracefully
  status     say whether the service is running, its health, its leases and idle time
  doctor [<repo>]   check git, the data folder and modes, the service, the store, optional tools, and that hooks find formslot; exits 1 on a real problem
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
  eval [--gold F] [--corpus C] [--arm formslot|baseline|dense|all] [--k 1,4,10] [--half all|tuning|held-out] [--json] [--out F]   recall, MRR, nDCG, tokens and latency over a gold set
  eval --arm model [--sample N] [--model M] [--runs R] [--seed S] [--jobs J] [--max-cost USD] [--out F]   the only arm that calls a model: claude -p with and without the pack, pass rate, tokens, cost (uses your Claude Code login)
  mcp        serve MCP over stdio (newline-delimited JSON-RPC) for an agent's .mcp.json
  check [--mode warn|deny|off]      PreToolUse hook: reads the hook JSON on stdin, answers declines and reuse
  hook <event>                      Claude Code hook: SessionStart, UserPromptSubmit, PostToolUse, PreCompact, SubagentStart, Stop
  install <repo> [--mode warn|deny] [--enable] [--dry-run]   merge the hooks and the MCP entry into a repository, keeping every other key
  version    print the version
  help       this text

  FORMSLOT_PORT       the port (5645); 0 asks the system for a free one
  FORMSLOT_DATA_DIR   the data folder (~/.formslot)
  FORMSLOT_IDLE_MINUTES   stop the service after this many minutes with no lease and no request (10); 0 turns it off
  FORMSLOT_IDLE_SECONDS   the same in seconds, for checking; takes precedence
  FORMSLOT_LEASE_SECONDS  how long a lease lasts without a heartbeat (90)
  FORMSLOT_<NAME>_BIN     the path of an optional program: GITLEAKS, RG or CURL
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
    if args.len() > 1 && args[1..].iter().any(|a| a == "--help" || a == "-h") {
        print!("{HELP}");
        return ExitCode::SUCCESS;
    }
    match command {
        "serve" => service_cmd::serve(&args[1..]),
        "start" => service_cmd::start(),
        "stop" => service_cmd::stop(),
        "status" => service_cmd::status(),
        "doctor" => service_cmd::doctor(&args[1..]),
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


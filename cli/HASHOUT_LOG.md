# HASHOUT_LOG · cli · formslot

Living log for `formslot-cli`. Source files stay comment-free; what they would have said lives here.

Current build version: v0.10.1
Last updated: 2026-10-30 · Phase S5

## Org-navi-map

```text
cli/
  Cargo.toml        package manifest, binary formslot-cli
  HASHOUT_LOG.md    this log
  src/
    main.rs         argument parsing, serve, status, doctor
    corpus_cmd.rs   add, reindex, stats, files, family
    symbols_cmd.rs  symbols, show, refs
    dedup_cmd.rs    dupes (and --related), drift
    slot_cmd.rs     slot list, get, seed, refresh, propose, confirm, decline, plan, apply, drift (log: core/src/slots/HASHOUT_LOG.md)
```

## Table of contents

- main.rs
- corpus_cmd.rs
- symbols_cmd.rs
- dedup_cmd.rs

## main.rs

### main.rs · dispatch
- refers-to: cli/src/main.rs :: main
- kind: explainer
- logged: v0.2.0 · 2026-10-30

Arguments are parsed by hand, the way formMCP does. One match on the first word sends to `serve`, `status`, `doctor`, `version` and `help`, and to the S2 commands `add`, `reindex`, `stats`, `files` and `family` in `corpus_cmd.rs`; any other word prints usage to stderr and exits 2. Every command calls into `formslot-core`; the shell stays thin.

### main.rs · serve
- refers-to: cli/src/main.rs :: serve
- kind: rationale
- logged: v0.2.0 · 2026-10-30

Takes the lock first. If another process holds it, the discovery file is read and `formslot is already running on 127.0.0.1:<port> (pid <pid>)` is printed with exit 0, so a hook or script can call `serve` without checking. If this process holds the lock, a discovery file left by a dead pid is removed, the listener is bound on `config::port()`, the file is written, and the call blocks in `Server::wait`. The lock `File` stays alive until the function returns.

### main.rs · serve and signals
- refers-to: cli/src/main.rs :: serve
- kind: warning-context
- logged: v0.2.0 · 2026-10-30

Std has no signal handling, so on Ctrl-C or SIGTERM the process ends without running the removal line and `service.json` stays on disk. That is safe: the operating system frees the lock, `status` and `doctor` check the pid, and the next `serve` removes the stale file. Clients must check `pid` as the contract says. Removal on exit through a handler is a todo.

### main.rs · status
- refers-to: cli/src/main.rs :: status, health
- kind: explainer
- logged: v0.2.0 · 2026-10-30

Reads the discovery file, checks the pid, then sends `GET /api/health` with the bearer token over a raw `TcpStream` with a proper Host header and prints one line. Exit 0 when the service answers, 1 when it is not running or does not answer, so scripts can test it. The token is used only in that request and never printed.

### main.rs · doctor
- refers-to: cli/src/main.rs :: doctor
- kind: explainer
- logged: v0.2.0 · 2026-10-30

Prints the version and contract, the data folder, the configured port, whether `git` is on PATH with its version (formslot needs it to read repositories), and the discovery state. It never prints the token. Rust is not checked: it is only needed to build, and the launcher says so.

## corpus_cmd.rs

### corpus_cmd.rs · commands
- refers-to: cli/src/corpus_cmd.rs :: add, reindex, stats, files, family
- kind: explainer
- logged: v0.2.0 · 2026-10-30

Each command opens the store at `config::corpus_db()` directly and calls the `corpus` or `family` function of the same name, so the command line works with or without a running service; the service and the command line share the same SQLite file under WAL. `add` and `reindex` print one line: name, the first 12 characters of the head, and the IndexRun counters. `stats` prints a language table, the role counts, the licences and the secret finding count, or the raw Stats with `--json`. `files` prints one path per line. `family --family-json <path>` writes the family table from formMCP's generated list and `--check` exits 1 when the table differs from the file. The first positional argument that is not an option value is the path or corpus name. An option given without its value (`--ref`, `--role`, `--language`, `--family-json` last, or followed by another option) is an error that names the option, rather than falling back to a default (v0.2.1, found in review).

### corpus_cmd.rs · verification
- refers-to: cli/src/corpus_cmd.rs :: add, stats
- kind: warning-context
- logged: v0.2.0 · 2026-10-30

Run on 2026-10-30 against formvis v1.98.4 at 8fe4ce45: 1997 files listed, 1789 read, 2612 ms in a release build; a second run read 0 in 30 ms (52 ms in v0.2.1, 887 ms in v0.4.0 because every run now rebuilds the edge and clone tables).

## symbols_cmd.rs

### cli/src/symbols_cmd.rs · symbols
- refers-to: cli/src/symbols_cmd.rs :: symbols
- kind: explainer
- logged: v0.4.0 · 2026-10-30

`formslot symbols <corpus> [--lang L] [--kind K] [--exported] [--count]` lists one line per unit as `path:line kind qualified`, ordered by path and line. Only files with role source, data or doc are listed, the same set the graph uses. `--count` prints the number only. The language is the S2 language string (ts, tsx, js, py, rs, css, html).

### cli/src/symbols_cmd.rs · show
- refers-to: cli/src/symbols_cmd.rs :: show
- kind: explainer
- logged: v0.4.0 · 2026-10-30

`formslot show <corpus> <path>#<qualified>` splits at the first `#`, so paths must not contain one. It prints each matching unit (CSS custom properties redefined per theme match more than once): line range, stable id, exported, signature, the references made inside its line range, the edges that start at it, and the edges that end at it with the number of files. Each list is cut at 60 rows with a "more" line.

### cli/src/symbols_cmd.rs · refs
- refers-to: cli/src/symbols_cmd.rs :: refs
- kind: explainer
- logged: v0.4.0 · 2026-10-30

`formslot refs <corpus> <name>` prints every edge whose target qualified name equals the name or ends in `.name` or `::name`, as `from_path:line kind -> to_path#to_qualified`. A bare method name therefore finds the method on every class. The name is taken as the second argument as written, so a CSS custom property (`formslot refs formvis --accent-color`) is not mistaken for an option; a lone `--` is skipped.

### cli/src/symbols_cmd.rs · queries
- refers-to: cli/src/symbols_cmd.rs :: rows; cli/src/symbols_cmd.rs :: lit
- kind: rationale
- logged: v0.4.0 · 2026-10-30

The queries are built as SQL text with quote-doubled literals and run with no bound parameters. Every selected column is cast to text in SQL and read back as a string. Literal values come only from the command line of the person running the command against their own database. The cli now depends on rusqlite (for the dedup commands), so moving these queries to bound parameters is a small later change.

## dedup_cmd.rs

### cli/src/dedup_cmd.rs · dupes
- refers-to: cli/src/dedup_cmd.rs :: dupes, print_groups, print_pairs
- kind: explainer
- logged: v0.4.0 · 2026-10-30

`formslot dupes <corpus>` reads the clone tables the engine built with `dedup::link`; it never recomputes anything. It prints the clone groups that have at least two members inside the corpus, then every pair whose two sides are both inside it, grouped by tier (T1, T2, T3). "Largest first" means the longest unit in lines, because the tables hold line ranges, not token counts. `--across <A> <B>` keeps groups that have a member in each corpus and pairs whose sides are in the two corpora, in either order. `--limit N` caps the group and pair lines per run (200 by default, 0 for all; since v0.4.1 0 also means all for groups, which used to bind `LIMIT 0` and print none); the line shows the true total at the end. A corpus is resolved by name or by numeric id with one query on `corpus`; since v0.4.1 an exact name wins over an id, as in `symbols_cmd.rs`, so a corpus named "2" and the corpus numbered 2 resolve the same way every time.

Pair line format: `T2 score 1.00  path:start-end qualified  <->  path:start-end qualified`. For T1 and T2 the score is always 1.00; for T3 it is the estimated Jaccard of the two token signatures. Group and pair lines can show a unit against a copy of itself in another corpus under the same path; that is the usual case for shared family code.

### cli/src/dedup_cmd.rs · drift
- refers-to: cli/src/dedup_cmd.rs :: drift
- kind: explainer
- logged: v0.4.0 · 2026-10-30

`formslot drift <A> <B>` calls `dedup::drift` live (it does not read the clone tables) and prints a count per state, then the rows that are not `identical` unless `--all` is given. The direction is A to B: every unit of A is matched to its best counterpart in B.

### cli · closed pipe
- refers-to: cli/src/main.rs :: main
- kind: rationale
- logged: v0.4.0 · 2026-10-30 · resolved v0.4.1

Every command prints with `println!`, which panics when the output pipe closes early (`formslot dupes x | head`). Since v0.4.1 `quiet_on_closed_pipe` in `main.rs` installs a panic hook that exits 0 when the panic is the broken pipe and hands every other panic to the default hook; std offers no way to reset SIGPIPE without a new dependency, and one hook covers every command. Per-language files and lines for ts, py, rs, css and js equal research B06's measurements exactly; md, json and html are lower by the generated and mirrored files that are listed but not read. formMCP (96 files) and formskills (90 files) indexed in under 50 ms each. substrata v2.11.1 was refused because the upload has no `.git`.

### cli/src/dedup_cmd.rs · print_related

- refers-to: cli/src/dedup_cmd.rs :: print_related
- kind: rationale
- logged: v0.5.0 · 2026-10-30

`dupes --related` prints `R <reason> <score>  path:start-end qualified  <->  path:start-end qualified`, grouped by name family (a `similar` row joins the family of whichever side has a `same_name` family, else its `a` side), families best score first with a blank line between, `same_name` rows first inside a family. Joining rows into connected components chained 15,877 formvis pairs into one blob, so families are used instead. With no `--limit` it prints every pair: the 200-line default buried the colour rows behind the `constructor` and `run` families. `--limit N` counts pairs.

### cli/src/eval_cmd.rs · run

- refers-to: cli/src/eval_cmd.rs :: run
- kind: explainer
- logged: v0.10.0 · 2026-10-30

Thin shell over `formslot_core::eval`: parses flags, prints the table and gate lines (or JSON), writes the Markdown report to `--out`, creating the parent folder.

### cli/src/dense_cmd.rs · run, search

- refers-to: cli/src/dense_cmd.rs :: run, search
- kind: rationale
- logged: v0.10.0 · 2026-10-30

In a build without the feature the `dense` command and `search --dense` exit 1 saying how to build it, so the flag never silently does nothing. `dense fetch` without `--yes` prints the URLs, sizes, digests, licence and target folder, downloads nothing and exits 1. Besides `pack --count remote`, no command other than `dense fetch --yes` touches the network; `dense build`, `search` and `search --dense` read only local files.

### cli/src/search_cmd.rs · pack --count remote

- refers-to: cli/src/search_cmd.rs :: pack
- kind: rationale
- logged: v0.10.0 · 2026-10-30

The key is looked up before the pack is built, so a missing key fails fast with exit 1; the remote count is taken after the pack, printed beside the local figures and added as `remoteTokens` to `--json`. A failed request keeps the local estimate and says so on stderr.

### cli/src/service_cmd.rs · serve, start, stop, status, doctor

- refers-to: cli/src/service_cmd.rs :: serve, start, stop, doctor
- kind: rationale
- logged: v0.10.2 · 2026-10-30

These moved out of main.rs with the new lifecycle commands. `serve` installs the `ctrlc` handler (feature `termination`, so SIGTERM and SIGHUP as well as SIGINT; Ctrl+C and Ctrl+Break on Windows) only after the lock is held, and the handler only sets the core stop flag; the main thread owns the cleanup, which is the same path as an idle stop or `POST /api/shutdown`, so service.json and the lock are always removed in one place. `start` spawns this same executable with `serve` and null streams, its own process group on Unix and DETACHED_PROCESS, CREATE_NEW_PROCESS_GROUP and CREATE_NO_WINDOW on Windows, and judges success by a live discovery file plus a health answer, not by the child's exit. A child that loses the lock race exits 0; the parent then finds the other service live. `stop` asks over HTTP; it never signals by pid, which keeps it portable. `doctor` opens the store read-only on purpose: doctor must not create or migrate anything. It reports `user_version` only, because the expected number is private to store.rs; the REPORT proposes a public constant there. Exit 1 only for a real problem (git missing, folder not writable, a live pid that does not answer, an unreadable discovery file or store, a repository hook that cannot be found).

### cli/src/eval_cmd.rs · run

- refers-to: cli/src/eval_cmd.rs :: run
- kind: explainer
- logged: v0.10.2 · 2026-10-30

New `--half all|even|odd` (also `tuning`, `heldout`) filters the gold set by line parity before the run, so the tuning and held-out halves can be reported from the same command. Default is all.

### cli/src/service_cmd.rs · stop, status, health

- refers-to: cli/src/service_cmd.rs :: stop
- kind: rationale
- logged: v0.10.2 · 2026-10-30

A stale `service.json` (dead or reused pid, nothing or something else on its port) made `stop` wait 15 seconds and fail, and sent the stale token to whatever listened (checked with a throwaway listener on the recorded port: the old binary delivered the Authorization header and took 15 s; the new one sends nothing and exits 0 in 3 ms). `stop` now takes the service lock: if it can be taken no service runs and the stale file is removed. With the lock held it needs a live pid and `health`, which first sends a request without a token and requires the 401 `unauthorized` JSON that only formslot answers, then the authenticated health; only then it posts the shutdown and requires 202. `status` uses the same `health`, and doctor reports a stale file whose pid was reused as a warning when the lock is free. `serve` calls `lifecycle::drain` before it removes the discovery file.

### cli/src/hook_cmd.rs · stdin_text

- refers-to: cli/src/hook_cmd.rs :: stdin_text
- kind: warning-context
- logged: v0.10.2 · 2026-10-30

`check` and `hook` read stdin to the end, so run by hand in a terminal they waited for ever. stdin is read only when it is not a terminal (`std::io::IsTerminal`); `main` answers `--help` and `-h` anywhere after the command before any command runs, so help never reads stdin. `add` was named in the review but does not read stdin (it errors at once without a path); `add --help` now prints the help.

### cli/src/eval_cmd.rs · run

- refers-to: cli/src/eval_cmd.rs :: run
- kind: rationale
- logged: v0.10.2 · 2026-10-30

`--half` is now `all|tuning|held-out` (`even` and `odd` are gone: line parity put neighbouring tasks, which share files and commits, in both halves). The split is by cluster; see core/src/eval.rs.

### cli/src/eval_cmd.rs · run_model

- refers-to: cli/src/eval_cmd.rs :: run_model
- kind: rationale
- logged: v0.11.0 · 2026-10-30

`--arm model` is handled before `Arm::parse` so it can never be reached through `--arm all` and eval.rs needs no new variant; it prints the model report's own table and gate lines and writes its own Markdown with `--out`. The work is in core/src/eval_model.rs.

### cli/src/eval_cmd.rs · run_model (--max-cost)

- refers-to: cli/src/eval_cmd.rs :: run_model
- kind: rationale
- logged: v0.11.0 · 2026-10-30

`--max-cost` is refused unless it is finite and above zero: NaN and negative caps made the spend check never or always fire. The engine repeats the check, so a library caller gets the same refusal.

### cli/src/setup_cmd.rs · run

- refers-to: cli/src/setup_cmd.rs :: run
- kind: rationale
- logged: v0.13.0 · 2026-10-30

The wizard holds only prompts and wording; tool checks, binary placement, shell file edits and path cleaning are in core/src/setup.rs. Services and connections go through the program's own commands (`start`, `status`, `install`, `doctor`) run as child processes of the installed binary rather than copies of their code: `start` needs `current_exe` to be the binary that should serve, and `install` writes `formslot` or an absolute path depending on that binary's PATH, so the child gets PATH with the program folder in front only when the folder is on PATH or was just added to the shell file. "Connected" is `install --dry-run` reporting nothing to change, run the same way, so a second setup agrees with the first. `--dry-run` and `--yes` imply no prompts; a missing terminal without either exits 2 before anything is touched. Port 5645 being busy shows up as a start failure with the start command's own message; setup carries on, since indexing does not need the service.

### cli/src/setup_cmd.rs · arm_interrupt, critical

- refers-to: cli/src/setup_cmd.rs :: critical
- kind: warning-context
- logged: v0.13.0 · 2026-10-30

Ctrl+C prints one sentence and exits 130 at once, except inside `critical`: the binary copy (temporary name then rename) and the shell file append (backup first, one write) set a busy flag, and an interrupt that arrives then is acted on after the step finishes. The `install` child runs in its own process group so the terminal's SIGINT does not stop it halfway. An interrupted index run is not protected the same way: the store is SQLite and the next run reads again only what is missing. End of input on a prompt counts as leaving. Checked with a pseudo-terminal: Ctrl+C at the shell-file question left the copied binary in place and the shell file unchanged.

### cli/src/main.rs · bare command

- refers-to: cli/src/main.rs :: main
- kind: rationale
- logged: v0.13.0 · 2026-10-30

No arguments used to print the help. It now offers setup or a status only when stdin and stdout are both terminals, so pipes, cron and scripts still get the help text and its exit code. "Set up" means at least one indexed project in the store, read with a read-only connection so asking creates nothing.

### cli/src/setup_cmd.rs · step_path (shell file)

- refers-to: core/src/setup.rs :: add_to_rc
- kind: warning-context
- logged: v0.13.0 · 2026-10-30

The PATH line is shown first and appended only after a yes (or `--yes` without `--no-path`). The file is backed up as `<name>.formslot-backup` before the append, and a marker comment line makes a second run recognise the edit. The append goes through the path itself, not a rename, so a shell file that is a symlink into a dotfiles repository keeps being a symlink. No sudo anywhere. Windows runs one PowerShell command that adds the folder to the user PATH only when it is not listed.

# HASHOUT_LOG · cli · formslot

Living log for `formslot-cli`. Source files stay comment-free; what they would have said lives here.

Current build version: v0.2.1
Last updated: 2026-10-30 · Phases S1 and S2

## Org-navi-map

```text
cli/
  Cargo.toml        package manifest, binary formslot-cli
  HASHOUT_LOG.md    this log
  src/
    main.rs         argument parsing, serve, status, doctor
    corpus_cmd.rs   add, reindex, stats, files, family
```

## Table of contents

- main.rs
- corpus_cmd.rs

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

Run on 2026-10-30 against formvis v1.98.4 at 8fe4ce45: 1997 files listed, 1789 read, 2612 ms in a release build; a second run read 0 in 30 ms. Per-language files and lines for ts, py, rs, css and js equal research B06's measurements exactly; md, json and html are lower by the generated and mirrored files that are listed but not read. formMCP (96 files) and formskills (90 files) indexed in under 50 ms each. substrata v2.11.1 was refused because the upload has no `.git`.

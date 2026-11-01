# HASHOUT_LOG · core · formslot

Living log for `formslot-core`. Source files stay comment-free; what they would have said lives here.

Current build version: v0.4.0
Last updated: 2026-10-30 · Phases S3 and S4

## Org-navi-map

```text
core/
  Cargo.toml        package manifest, dependencies
  HASHOUT_LOG.md    this log
  src/
    lib.rs          module list, Result, VERSION, CONTRACT
    config.rs       port and data folder
    discovery.rs    lock, service.json, token, pid check
    http.rs         the std-only loopback server and its checks
    api.rs          the route table: health and the corpus routes
    log.rs          the redacted JSON line log
    redact.rs       secret shapes and known values
    git.rs          the system git CLI: ls-tree, cat-file --batch, diff-tree
    store.rs        SQLite in WAL mode, forward-only migrations
    select.rs       language and role rules as data
    hygiene.rs      built-in secret rules, optional gitleaks
    licence.rs      SPDX header, licence file, manifest
    corpus.rs       add, reindex, list, stats, files: the service layer
    family.rs       the family list from formMCP's family.json
    facts.rs        per-blob facts: symbols then dedup prints; link after a run
    dedup.rs        lexer, T1 to T3 prints, clone links, drift
    symbols/
      mod.rs        Unit, Reference, Import, extract, stable_id, sig8
      typescript.rs ts, tsx and js walker
      python.rs     python walker
      rust.rs       rust walker, tauri commands
      css.rs        custom properties and selectors
      html.rs       element ids, inline script and style
      persist.rs    store: per-blob unit, ref and import rows
      graph.rs      link: per-corpus edges
    migrations/
      001_corpus.sql  corpus, blob, file, licence, findings, family, meta
      002_symbols.sql unit, unit_ref, unit_import, edge, blob.facts
      003_dedup.sql   unit_print, clone_group, clone_member, clone_pair
```

## Table of contents

- lib.rs
- config.rs
- discovery.rs
- http.rs
- api.rs
- log.rs
- redact.rs
- Cargo.toml
- serve and the discovery file
- git.rs
- store.rs
- migrations/001_corpus.sql
- select.rs
- hygiene.rs
- licence.rs
- corpus.rs
- family.rs
- facts.rs
- symbols
- migrations/002_symbols.sql
- dedup.rs
- migrations/003_dedup.sql

## lib.rs

### lib.rs · module list
- refers-to: core/src/lib.rs :: Result, VERSION, CONTRACT
- kind: rationale
- logged: v0.2.0 · 2026-10-30

`Result` carries a plain `String` error, the family pattern from formMCP: every message is a sentence a person can read, and nothing in an error may hold a secret. `CONTRACT` is the contract number in `docs/CONTRACT.md` and is returned by `/api/health` so a client can refuse a service that speaks another number. The S1 file lists only the modules that exist in S1; the corpus modules are added to it by hand as they land.

## config.rs

### config.rs · port
- refers-to: core/src/config.rs :: port, DEFAULT_PORT
- kind: explainer
- logged: v0.2.0 · 2026-10-30

5645 is formslot's HTTP port in the family block 5640 to 5649. `FORMSLOT_PORT` overrides it; `0` is allowed and asks the system for a free port, which is how a second data folder or a test run avoids a clash. A value that is not a number falls back to 5645. The port actually bound is what the discovery file holds.

### config.rs · data_dir
- refers-to: core/src/config.rs :: data_dir, corpus_db, log_file
- kind: explainer
- logged: v0.2.0 · 2026-10-30

`FORMSLOT_DATA_DIR`, else `~/.formslot` (`USERPROFILE` on Windows). The folder is created on first use with mode 0700 on unix. Files inside get 0600 where they are created (lock, discovery, log); `corpus.sqlite` gets its mode from the store in S2.

## discovery.rs

### discovery.rs · lock
- refers-to: core/src/discovery.rs :: acquire_lock
- kind: rationale
- logged: v0.2.0 · 2026-10-30

One service per data folder is enforced by an exclusive advisory lock on `service.lock`, using `File::try_lock` from std (stable since Rust 1.89, so no crate and no libc). The operating system drops the lock when the process dies, so a crash never leaves a stuck lock. `Ok(None)` means another process holds it. The caller must keep the returned `File` alive for as long as the service runs.

### discovery.rs · write
- refers-to: core/src/discovery.rs :: write, read, remove
- kind: explainer
- logged: v0.2.0 · 2026-10-30

`service.json` is written to `service.json.tmp` with mode 0600, synced, then renamed over the real name, so a reader never sees half a file and the token is never readable by another user, not even briefly. It is written after the listener is bound so the port in it is the real one. `read` refuses a file whose `format` is not `formslot.service`.

### discovery.rs · pid_alive
- refers-to: core/src/discovery.rs :: pid_alive
- kind: rationale
- logged: v0.2.0 · 2026-10-30

No libc crate: on Linux a process exists when `/proc/<pid>` does; elsewhere on unix `ps -p <pid>` decides; on Windows `tasklist` is searched for the pid. A pid can be reused by another program after a crash, so a live pid is evidence, not proof; the token and the health call are what prove the service.

### discovery.rs · now_rfc3339
- refers-to: core/src/discovery.rs :: now_rfc3339
- kind: explainer
- logged: v0.2.0 · 2026-10-30

UTC time as RFC 3339 without a date crate, using the civil-from-days algorithm (days since 1970-01-01 shifted to 0000-03-01, 400-year eras). Whole seconds only.

### discovery.rs · new_token
- refers-to: core/src/discovery.rs :: new_token
- kind: warning-context
- logged: v0.2.0 · 2026-10-30

32 bytes from the system random source through getrandom 0.4 (`getrandom::fill`), hex encoded to 64 characters, new on every start. If the system source fails the function stops the program rather than fall back to a weak token.

## http.rs

### http.rs · server shape
- refers-to: core/src/http.rs :: start, Server, handle
- kind: rationale
- logged: v0.2.0 · 2026-10-30

Adapted from formMCP's `http.rs` (GPL-3.0-only, same owner): a std `TcpListener` bound to 127.0.0.1 only, polled non-blocking so `Server::stop` can end it, one thread per connection, 30 second read timeout, `Connection: close`. No async runtime and no HTTP crate. Port 0 binds a free port and `Server.port` reports the real one. The router is a plain `fn(&Request) -> Reply` so the engine owns every route and the server owns every check.

### http.rs · checks and their order
- refers-to: core/src/http.rs :: handle, host_ok, origin_ok, same
- kind: rationale
- logged: v0.2.0 · 2026-10-30

Order: Host (421), Origin when present (403), bearer token (401), then the route. The Host check stops DNS rebinding, the Origin check stops a web page from calling the service, and the token stops other local programs. Host and Origin are compared in full against `127.0.0.1:<port>` and `localhost:<port>` (with `http://` for Origin) using the port actually bound. The token is compared in constant time over equal-length strings. The token is also registered with the redactor so a stray copy in any logged string is hidden.

### http.rs · limits
- refers-to: core/src/http.rs :: read_request, BODY_MAX, HEADERS_MAX
- kind: warning-context
- logged: v0.2.0 · 2026-10-30

Bodies are at most 1 MiB (413 above that) and header lines together at most 64 KiB. Each line is read through a `take` of the room left, so a single line without a newline cannot grow past the limit before it is refused (v0.2.1; before that `read_line` buffered the whole line first). Chunked bodies are refused with 400: every client sends a Content-Length. A request target that does not start with `/` is 400. Host, Origin, Authorization, Content-Length and Transfer-Encoding may each appear once; a second copy is 400, so a request cannot pass with a forged first Host and a good last one (v0.2.1). Errors are always `{"error","message"}`.

### http.rs · connections
- refers-to: core/src/http.rs :: start
- kind: todo
- logged: v0.2.1 · 2026-10-30

One thread per connection with a 30 s read timeout that restarts on every read and no cap on connections. Any local program can hold many slow connections open. A connection cap and a total per-request deadline are planned for the phase that exposes `/mcp` (S8), when clients other than the command line and subform's engine start connecting.

### http.rs · what is logged
- refers-to: core/src/http.rs :: refuse
- kind: warning-context
- logged: v0.2.0 · 2026-10-30

A refusal logs the reason word and the status only. The token, the body and the query string are never given to the log.

## api.rs

### api.rs · route
- refers-to: core/src/api.rs :: route
- kind: explainer
- logged: v0.2.0 · 2026-10-30

One match on the method and the path split into segments, so `/api/corpus/<id>/stats` binds the id without a router. Routes: `GET /api/health`, `GET /api/corpus`, `POST /api/corpus` (body `path`, optional `ref`), `POST /api/corpus/<id>/reindex`, `GET /api/corpus/<id>/stats`; everything else 404. The id segment is a corpus name or its number, passed straight to `corpus`. A missing or non-JSON body on a POST is 400, and so is a `path` that is not absolute (a relative one would resolve against the service's working folder; v0.2.1); an engine error is 422 with the engine's sentence. Not done yet (review, low): an unknown corpus is 422 rather than 404, and a known path with the wrong method is 404 rather than 405; both wait for the contract revision that S8 brings. Each request opens the store itself (a rusqlite connection is not Sync, and WAL lets readers run beside a writer); index runs take the `INDEXING` mutex so two runs never hold write transactions at once. Indexing answers only after the run, which takes about 2.6 s for formvis in a release build.

## log.rs

### log.rs · record
- refers-to: core/src/log.rs :: record
- kind: explainer
- logged: v0.2.0 · 2026-10-30

One JSON object per line in `formslot.log` with mode 0600, fields `t` (milliseconds), `event`, `pid` and the caller's fields, every string passed through `redact::value` before it is written. At 1 MiB the file is renamed to `formslot.1.log` and a new one starts. Write failures are ignored: logging must never stop the service.

## redact.rs

### redact.rs · shapes and known values
- refers-to: core/src/redact.rs :: SHAPES, remember, text, value
- kind: rationale
- logged: v0.2.0 · 2026-10-30

Copied from formMCP's `redact.rs` with the mark changed to `[redacted by formslot]` and without formMCP's skipping of `blob` and `data` keys, since formslot has no base64 media to spare. Known values (the access token, 8 characters or more) are replaced first, longest first; then token shapes: private key blocks, `fmd_`, Anthropic and OpenAI keys, GitHub tokens, Slack, AWS, Google, JWTs, and a keyword (bearer, token, api key, secret, password) followed by 20 or more token characters, which keeps the keyword. A bare 64-hex string is not a shape, so commit ids and SHA-256 digests are never hidden. Regex keeps its default features: without them the `(?i)` shape is dropped silently.

## Cargo.toml

### Cargo.toml · dependencies
- refers-to: core/Cargo.toml :: dependencies
- kind: rationale
- logged: v0.2.0 · 2026-10-30

serde and serde_json for the wire; sha2 0.10 and rusqlite 0.41 (bundled SQLite, so no system library) are for the S2 store and are listed now so the lockfile is stable; getrandom 0.4 for the token; regex 1 for redaction. sha2 stays on 0.10 to match the family; 0.11 exists. Edition 2024 and rust-version 1.90 come from the workspace; `File::try_lock` needs 1.89.

## serve and the discovery file

### cli serve · exit and the discovery file
- refers-to: cli/src/main.rs :: serve
- kind: warning-context
- logged: v0.2.0 · 2026-10-30

Removal of `service.json` when the service stops cannot be done on SIGTERM or Ctrl-C with std alone: there is no signal handler in std and no libc or signal crate is allowed in S1. The file is removed when `Server::wait` returns, which only happens on an orderly stop. After a kill the file stays; `formslot status` reports it stale by the dead pid, and the next `serve` removes it once it holds the lock. A signal handler is a todo for a later phase.

## git.rs

### core/src/git.rs · Repo
- refers-to: core/src/git.rs :: Repo
- kind: rationale
- logged: v0.2.0 · 2026-10-30

Every git read goes through the system git CLI with std::process::Command: no git library is linked, so the git licence (GPL-2.0 only) never reaches the formslot binary, and what an agent sees from git is exactly what formslot indexes. Each call sets GIT_TERMINAL_PROMPT=0, LC_ALL=C and GIT_OPTIONAL_LOCKS=0 and passes the repository with -C. Revisions that came from outside are always passed after --end-of-options and peeled with ^{commit}, so a revision name can never be read as an option. Nothing writes to the repository. `remote_url` passes the origin URL through `without_credentials`, which drops any `user:password@` part of a `scheme://` URL before it is stored or returned, so a token embedded in a remote never reaches the database or an API answer (v0.2.1, found in review); scp-style `git@host:path` remotes have no credentials and are kept as they are. A folder that is not a git repository (substrata v2.11.1 as uploaded has no `.git`) is refused with git's own message; formslot indexes commits, not loose folders.

### core/src/git.rs · Repo::ls_tree
- refers-to: core/src/git.rs :: Repo::ls_tree
- kind: explainer
- logged: v0.2.0 · 2026-10-30

Runs `git ls-tree -r -z -l` and splits on NUL, then on the first TAB: left of it are mode, kind, oid and the padded size, right of it is the raw path. The -l form is used instead of --format because %(objectsize) as a format atom was not verified on the minimum git (B03). Submodule entries (mode 160000) and anything that is not a blob are dropped here, so no later stage can read a submodule by mistake.

### core/src/git.rs · decode_path
- refers-to: core/src/git.rs :: decode_path
- kind: warning-context
- logged: v0.2.0 · 2026-10-30

Git paths are bytes, not text. The path is stored as lossy UTF-8; when lossy conversion changed anything, the original bytes are kept as lowercase hex in `path_hex` (an Option, None for the normal case). Two different non-UTF-8 paths can collapse to the same lossy string; the file table keys on the lossy path, so one of the two would win. None exists in the test corpora; the hex column is there so a later phase can detect and handle it.

### core/src/git.rs · BlobReader
- refers-to: core/src/git.rs :: BlobReader
- kind: explainer
- logged: v0.2.0 · 2026-10-30

One long-lived `git cat-file --batch` child per index run. The reader writes `<oid>\n`, reads the `<oid> <type> <size>` header line, then exactly `size` bytes and the trailing LF; a `missing` header yields None. The process is killed and reaped on drop, so a failed run leaves no child behind. Blobs are read whole into memory; the caller refuses anything over 32 MiB before asking.

### core/src/git.rs · Repo::changed
- refers-to: core/src/git.rs :: Repo::changed
- kind: explainer
- logged: v0.2.0 · 2026-10-30

`git diff-tree -r --raw --no-renames -z <from> <to>` emits records `:oldmode newmode oldoid newoid status` and then the path as a separate NUL token. Because it compares two trees, ancestry does not matter, so a force-pushed or rebased head needs no special case. The raw layout was marked "not verified" in B03; it was checked against a scratch repository with an added, a modified and a deleted file. Old and new modes are returned so the caller can tell a file turned into a submodule.

## store.rs

### core/src/store.rs · Store
- refers-to: core/src/store.rs :: Store::open
- kind: rationale
- logged: v0.2.0 · 2026-10-30

One SQLite file, opened in WAL mode with foreign keys on, synchronous NORMAL and a 5000 ms busy timeout (A04). The file is created with mode 0600 before SQLite touches it, and SQLite gives the -wal and -shm files the same mode. Migrations are forward-only SQL strings embedded with include_str!, applied one transaction each and recorded in `PRAGMA user_version`; there is no migration framework. A later phase that needs FTS or vectors adds 002 and later files to the MIGRATIONS array.

## migrations/001_corpus.sql

### core/src/migrations/001_corpus.sql · blob
- refers-to: core/src/migrations/001_corpus.sql :: blob
- kind: rationale
- logged: v0.2.0 · 2026-10-30

`blob` is keyed on (object_format, oid), never oid alone, because the same bytes hash differently in a SHA-1 and a SHA-256 repository (B03). The sha256 column is the cross-format content digest. `spdx_header` and `spdx_doc` cache what was learned when the blob was read (an SPDX header in the file, or the licence named by a LICENSE or manifest file) so a re-index can rebuild every file's licence without reading anything again. `blob_finding` holds secret-scan findings per blob and never any secret text: only the rule id, the line and a sha256 fingerprint of the matched string. The per-corpus `scan_finding` table is rebuilt from it by joining on the files of the corpus.

### core/src/migrations/001_corpus.sql · file
- refers-to: core/src/migrations/001_corpus.sql :: file
- kind: explainer
- logged: v0.2.0 · 2026-10-30

`file` is the manifest of one corpus at its head: one row per tracked path with oid, mode, size, language, role and licence. `commit` is quoted because it is an SQL keyword. Rows for generated, mirror and binary-by-extension files exist with no matching blob row (they are listed, not read), so every join to `blob` is a LEFT JOIN and their line count counts as zero. `licence` and `scan_finding` are derived tables and are rebuilt on every index run.

## select.rs

### core/src/select.rs · SKIP_RULES
- refers-to: core/src/select.rs :: SKIP_RULES
- kind: rationale
- logged: v0.2.0 · 2026-10-30

Role rules are data: an array of Rule values (prefix, directory segment, suffix, exact names, excluded names), first match wins, and the array order is the precedence. SKIP_RULES hold the path-only roles that decide whether a blob is read at all: mirror (`.agents/skills/` only while `.claude/skills/` exists in the same tree), generated (lockfiles, the generated skill references commands.md, tools.md, tool-map.md and command-map.md, licenses/*.html, minified files) and vendored (third_party, vendor, node_modules). Generated and mirror files are the token traps in B06: the skill mirror alone is 523 KB stored twice, and commands.md is 233 KB. Vendored files are read, since only reading generated, mirror and binary-by-extension files is skipped.

### core/src/select.rs · CONTENT_RULES
- refers-to: core/src/select.rs :: CONTENT_RULES
- kind: explainer
- logged: v0.2.0 · 2026-10-30

Applied only after the skip rules and the binary test: log (CHANGELOG.md, HASHOUT_LOG.md anywhere), data (TypeScript files under an i18n directory except index.ts and loader.ts, and JSON files under a data directory), doc (anything under docs or .docs, and md, mdx and txt files) and then source for everything else. A file is binary when its extension is in the binary table, or when a NUL byte appears in its first 8000 bytes once read. Binary-by-extension files are not read; this is a deliberate extension of the "generated and mirror are not read" rule so images, fonts and model weights are never pulled through the pipe.

### core/src/select.rs · language
- refers-to: core/src/select.rs :: language
- kind: explainer
- logged: v0.2.0 · 2026-10-30

Language is the extension looked up in a table (ts, tsx, js, py, rs, css, html, md, json, toml, yaml, sh, wgsl); everything else, including files without an extension such as the launcher scripts, is "other". It is independent of role: a locale table is language ts and role data.

## hygiene.rs

### core/src/hygiene.rs · scan_text
- refers-to: core/src/hygiene.rs :: scan_text
- kind: rationale
- logged: v0.2.0 · 2026-10-30

A built-in rule set, so indexing needs no external binary: AWS access keys, GitHub tokens (ghp_, gho_, ghu_, ghs_, ghr_ and github_pat_), Slack xox tokens, PEM private key headers, Anthropic sk-ant- keys, OpenAI sk- keys, fal keys (uuid, colon, 32 hex), a generic `api_key = "..."` assignment, and 40 or more hex characters within 24 characters after the whole word secret or token. A RegexSet over the whole text decides in one pass whether any line needs a closer look. A finding is the rule id, the 1-based line and the sha256 of the matched string; the match itself is dropped immediately and never stored, logged or returned. The first version of the hex rule matched "tokenizer" in the model-hash table of formvis (18 false findings); it now requires word boundaries around secret and token. Expect false positives from the generic rule on placeholder values in documentation.

### core/src/hygiene.rs · scan_with_gitleaks
- refers-to: core/src/hygiene.rs :: scan_with_gitleaks
- kind: todo
- logged: v0.2.0 · 2026-10-30

Optional and not part of the index run. When a gitleaks binary is in FORMSLOT_BIN_DIR or on PATH, `scan_with_gitleaks(tree)` runs `gitleaks dir <tree> --report-format json --report-path - --redact` over a whole working tree and returns path, rule, line and a fingerprint of path:rule:line (never the secret, because of --redact). It was not run in S2: gitleaks is not installed on the build machine, so the flags are from the B08 notes and gitleaks 8.30 usage and need one real run before the CLI exposes them. gitleaks is MIT and a separate process, so it needs only a NOTICE row if it is ever bundled.

## licence.rs

### core/src/licence.rs · Index::resolve
- refers-to: core/src/licence.rs :: Index::resolve
- kind: rationale
- logged: v0.2.0 · 2026-10-30

Order of evidence for one file: an SPDX-License-Identifier header in its first 30 lines (a line that is only a comment marker, the tag and an SPDX-shaped expression, so code that merely contains the words, such as this module's own regex, is not read as a header; v0.2.1), then the nearest LICENSE, LICENCE or COPYING file walking up the tree (matched against a small table by its first 3000 characters), then the nearest package.json, Cargo.toml or pyproject.toml license field, else "unknown" with source `unknown`. The sources are recorded as header, licence_file, manifest and unknown. Consequence of that order: a package.json that declares ISC inside a tree whose root LICENSE is MIT is itself reported as MIT, because the licence file wins over the manifest. Unknown means no copy; the verdict mapping is a later phase.

### core/src/licence.rs · licence_text_spdx
- refers-to: core/src/licence.rs :: licence_text_spdx
- kind: warning-context
- logged: v0.2.0 · 2026-10-30

Matching is a table of lowercase phrases that must all appear in the first 3000 characters after whitespace is collapsed. Order matters: AGPL, then LGPL 3 and 2.1, then GPL 3 and 2, then Apache, MPL, BSD-3 before BSD-2, MIT, ISC, Unlicense, CC0. It names the family and the version, nothing more: a licence file with extra clauses or an exception (for example GPL with a linking exception) is reported as the plain licence. The result is stored on the blob, so a blob first seen under a name that is not a licence or manifest file never gets a document licence, even if the same bytes are later seen as LICENSE.

### licence.rs · licence cached per blob
- refers-to: core/src/corpus.rs :: store_blob; core/src/licence.rs :: Index::resolve
- kind: todo
- logged: v0.2.1 · 2026-10-30

Found in review and left for S5, when slots first depend on licences: `spdx_doc` is computed with the path of whichever file read the blob first, while the blob row is keyed by object id only. Identical bytes at a path with a different licence-file or manifest role can get a stale document licence. The fix is to keep both interpretations (as a licence file and as a manifest) on the blob, or to resolve them per path in the licence rebuild.

## corpus.rs

### core/src/corpus.rs · index
- refers-to: core/src/corpus.rs :: index
- kind: explainer
- logged: v0.2.0 · 2026-10-30

One index run is one transaction. Order, after B08: resolve the revision, list the tree (ls-tree), decide which paths to touch (all of them, or only the diff-tree paths when the corpus already has a head of the same object format and the diff succeeds), then for each touched path: skip it if its role says not to read it, reuse it if the blob is already known, otherwise read it through the BlobReader, sniff binary, count lines, hash it, scan text for secrets, read any SPDX header and any licence or manifest content, and store the blob. An untouched path that is skipped by its role is counted as skipped; any other untouched path is looked up in `blob` and counted as reused only when its blob is stored, otherwise it is read now. That lookup matters when a file's skip status flips without the file changing (for example `.agents/skills` stops being a mirror when `.claude/skills` is deleted): before v0.2.1 such files were counted as reused and never read. A repeat run at the same commit still reports read 0. The run opens its transaction with BEGIN IMMEDIATE and re-reads the corpus head and object format inside it before planning, so a command-line run and a service run cannot plan against a stale head or fail with a busy snapshot (v0.2.1). Afterwards removed paths are deleted, every row's commit is set to the new head, roles are recomputed from path, tree and blob (so a mirror becomes a mirror when `.claude/skills` first appears), and the licence and scan_finding tables are rebuilt for the corpus. Counters: listed is the whole tree, read is blobs pulled from git, reused is files whose blob was already stored, skipped is files not read (generated, mirror, binary by extension, over 32 MiB, or missing), secrets is findings in newly read blobs.

### core/src/corpus.rs · Stats
- refers-to: core/src/corpus.rs :: Stats
- kind: warning-context
- logged: v0.2.0 · 2026-10-30

Stats follow the CONTRACT field names. Two points the contract leaves open: `corpus` is the whole Corpus object, not just its name; and lines are the sum of stored blob line counts, so generated, mirror and binary files contribute bytes but zero lines. Line counts count a final line without a newline, so they can exceed `wc -l` by one per such file. Language and role counts include every tracked file, whatever its role.

### core/src/corpus.rs · add
- refers-to: core/src/corpus.rs :: add
- kind: explainer
- logged: v0.2.0 · 2026-10-30

A corpus is named after the base name of the repository top level; adding a path that is already a corpus re-indexes it, and adding a different path whose base name is taken is an error rather than a silent rename. Lookup accepts the name or the numeric id, which is what the HTTP route `/api/corpus/<id>/...` passes. The remote is `remote.origin.url` and may be None.

## family.rs

### core/src/family.rs · load_family_json
- refers-to: core/src/family.rs :: load_family_json
- kind: explainer
- logged: v0.2.0 · 2026-10-30

Reads the formMCP family list (format `formmcp-family`, format_version 1) and keeps the eight fields id, app, name, kind, summary, repository, launcher and state; the extra keys (released, mcp) are ignored on purpose, so the stored list does not drift when formcompiler adds fields. The apps are sorted by id before comparison. The list is never hand-kept: `write_family` replaces the whole `family` table and `check` returns true only when the table equals the file exactly, which is what `formslot family --check` reports. Until a formcompiler checkout is available, `formslot family` reads formMCP's generated file (`--family-json`); `--compiler` arrives when the owner shares formcompiler.

## facts.rs

### core/src/facts.rs · index_blob
- refers-to: core/src/facts.rs :: index_blob, stale, VERSION
- kind: rationale
- logged: v0.4.0 · 2026-10-30

The one place where per-blob facts are made: S3's `symbols::extract` and `symbols::store`, then S4's `dedup::fingerprint` over the same units as plain spans and `dedup::store`, then `blob.facts` is set to `VERSION`. Facts belong to the blob, not to a corpus, so identical bytes in two repositories are extracted once. Languages without an extractor still get `facts = VERSION` so they are never re-read. `stale` is true when a stored text blob carries an older `VERSION`, which is what makes a database from v0.2.x pick up symbols on its next run without a separate migration step: bumping `VERSION` re-reads every text blob once. The two phases only meet here and in `link`; neither module imports the other.

### core/src/facts.rs · link
- refers-to: core/src/facts.rs :: link
- kind: explainer
- logged: v0.4.0 · 2026-10-30

Runs after every index run, inside its transaction: `symbols::link` rebuilds the corpus's `edge` rows from its file manifest, and `dedup::link` rebuilds the clone tables across all corpora, because a clone pair can span two repositories and a new head of one changes pairs with every other.

### core/src/corpus.rs · facts hook
- refers-to: core/src/corpus.rs :: store_blob, process_entry, index
- kind: explainer
- logged: v0.4.0 · 2026-10-30

`store_blob` calls `facts::index_blob` for every new text blob. `process_entry` reads a known text blob again when `facts::stale` says its facts are older than `facts::VERSION` and counts it as read. The untouched-path check in `index` counts a path as reused only when its blob is binary or already has current facts. After the licence rebuild, `index` calls `facts::link`. Measured on 2026-10-30: a database written by v0.2.1 re-read 1,786 text blobs on its first v0.4.0 run (7.4 s for formvis) and read 0 on the next.

## symbols

### core/src/symbols/mod.rs · extract
- refers-to: core/src/symbols/mod.rs :: extract
- kind: explainer
- logged: v0.4.0 · 2026-10-30

One parser per call, one tree-sitter walk per language module. The language string is the S2 string from `select::language`. `ts` uses the TypeScript grammar; `tsx` and `js` both use the TSX grammar because plain JavaScript files in the corpus carry JSX and TSX is a superset for them. Unknown languages return None, and `facts::index_blob` then marks the blob done so it is not retried. Grammar crates and tree-sitter were checked together: ABI 14 for typescript, tsx and html, 15 for python, rust and css, all accepted by tree-sitter 0.27.1.

### core/src/symbols/mod.rs · Cx::unit
- refers-to: core/src/symbols/mod.rs :: Cx::unit
- kind: rationale
- logged: v0.4.0 · 2026-10-30

A unit's signature is the source from the start of the unit to the start of its body, whitespace collapsed and cut at 300 characters. The body is what identity must survive editing, so everything before it (modifiers, name, generics, parameters, return type, decorators for Python, the `export` keyword for TypeScript) is the signature and feeds `sig8`. Consequence worth knowing: adding or removing `export` changes the stable id. For multi-declarator `const a = 1, b = 2` statements the second and later declarators start at the declarator, not the statement, so the first declarator's text does not leak into their signature.

### core/src/symbols/mod.rs · stable_id
- refers-to: core/src/symbols/mod.rs :: stable_id; core/src/symbols/mod.rs :: sig8
- kind: explainer
- logged: v0.4.0 · 2026-10-30

The id is `formslot <repo> <path>#<qualified>~<sig8>` per research A01. It is derived on demand from stored columns and never stored itself, so a change to the format needs no migration. sig8 is the first four bytes of sha256 over the whitespace-normalised signature, as eight hex characters. Two units with the same path, qualified name and signature (CSS theme overrides of one custom property, duplicate selectors) share an id; A01's `@n` ordinal suffix is not applied and is left to the consumer that needs unique ids (S5 slots).

### core/src/symbols/mod.rs · Cx::reference
- refers-to: core/src/symbols/mod.rs :: Cx::reference
- kind: rationale
- logged: v0.4.0 · 2026-10-30

References carry only name, kind and line. Which unit makes a reference is not stored; `link` finds it by line range, taking the smallest unit that contains the line. Identical (name, kind, line) triples are dropped at extraction, so two calls to one function on one line count once. This keeps `unit_ref` small (about 140,000 rows for formvis) at the price of losing per-line call counts.

### core/src/symbols/typescript.rs · run
- refers-to: core/src/symbols/typescript.rs :: run; core/src/symbols/typescript.rs :: export
- kind: explainer
- logged: v0.4.0 · 2026-10-30

Top-level statements only, plus class members and namespace bodies. `export { a, b as c }` without a source marks the local names a and b as exported after all declarations are seen, so a list placed before or after the declaration works. `export { x } from './y'` and `export * from` become Import records, not units. `export default function () {}` and `export default class {}` become units named `default`. Methods are never marked exported: `exported` means an `export` form, and a method is reachable only through its class. On formvis: 3,783 exported TS units, about 3,690 of them outside i18n tables, against the 3,716 lines that start with `export ` (B06 measured 3,724 on an earlier tree); `export {}`, `export *` and `export default` lines are imports or marks, not units.

### core/src/symbols/typescript.rs · variables
- refers-to: core/src/symbols/typescript.rs :: variables
- kind: rationale
- logged: v0.4.0 · 2026-10-30

A `const` becomes a unit when its value is an arrow function, function expression or generator (kind `function`), or when it is exported (kind `const`). Unexported plain constants and destructuring declarations are skipped on purpose: they are file-local data and would swamp the unit table. Class fields holding arrow functions become `method` units because that is how event handlers are written in this codebase.

### core/src/symbols/typescript.rs · references
- refers-to: core/src/symbols/typescript.rs :: references; core/src/symbols/typescript.rs :: call
- kind: explainer
- logged: v0.4.0 · 2026-10-30

One cursor walk over the whole tree. A call's name is the last identifier of the callee (`a.b.c()` gives c). `invoke('x')` and `invoke<T>("x")` with a string first argument produce an `invoke` reference named x and no `call` reference; a dynamic `import('./m')` becomes an Import. `new X()` is recorded as a `call` to X. A `type_identifier` is a `type` reference unless it is the declared name of its parent. Strings and template strings are scanned for `var(--x)` and produce `css_var` references with the line of the occurrence.

### core/src/symbols/python.rs · run
- refers-to: core/src/symbols/python.rs :: run; core/src/symbols/python.rs :: item
- kind: explainer
- logged: v0.4.0 · 2026-10-30

Top-level functions, classes, methods of top-level classes, and module-level assignments whose target is an UPPER_CASE name. Decorated definitions take their span and signature from the decorated node, so a FastAPI decorator line is part of the signature of the route function. `exported` is true unless the name starts with an underscore, which applies to methods too. Imports are collected anywhere in the file (function-local and `TYPE_CHECKING` imports included) because the walk visits every node. Numbers: formvis has 1,569 top-level defs and classes under backend/, matching B06; the whole repository has 4,080 because e2e/ and setup/ hold Python too.

### core/src/symbols/rust.rs · items
- refers-to: core/src/symbols/rust.rs :: items; core/src/symbols/rust.rs :: is_tauri
- kind: explainer
- logged: v0.4.0 · 2026-10-30

Items are read from the file and recursively from `mod` and `impl` and `trait` bodies. An `attribute_item` whose whitespace-stripped text starts with `#[tauri::command` marks the next item only; any other node clears the mark, and comments and inner attributes do not. That covers `#[tauri::command(rename_all = "snake_case")]`. Impl methods are qualified `Type::method` using the rightmost type name (generics and references stripped); `impl Trait for Type` methods are qualified by Type and are exported only if written `pub`. Qualified names of items inside a mod are `mod::item`. formvis has 26 `tauri_command` units.

### core/src/symbols/css.rs · visit
- refers-to: core/src/symbols/css.rs :: visit
- kind: rationale
- logged: v0.4.0 · 2026-10-30

Every `--name:` declaration is a `custom_property` unit and is marked exported, since any stylesheet can read it. Every rule set is a `selector` unit whose name and qualified name are the collapsed selector text; custom properties inside a rule have that rule as parent. `@media`, `@supports` and nested blocks are descended; keyframe steps are not. A custom property redefined for several themes yields several units with the same name, which is why the graph links `var(--x)` to the earliest definition only. For the value of the property, the value node counts as the body, so `--accent-color: #5b8cff` has the signature `--accent-color:` and a colour change does not change the stable id. formvis tokens.css holds 162 declarations of 150 distinct names.

### core/src/symbols/html.rs · element
- refers-to: core/src/symbols/html.rs :: element
- kind: explainer
- logged: v0.4.0 · 2026-10-30

Elements with an `id` attribute become `element_id` units named by the id; their signature is the start tag, not the whole element, because the element can span the file. Inline `<script>` and `<style>` blocks (no `src`) become `script` and `style` units with qualified name `script@<line>`. Inline script text is not parsed with the TypeScript grammar in this phase, so calls and `invoke` inside HTML scripts are not extracted. `var(--x)` is scanned in style attributes and in script and style bodies.

### core/src/symbols/persist.rs · store
- refers-to: core/src/symbols/persist.rs :: store
- kind: explainer
- logged: v0.4.0 · 2026-10-30

Facts are keyed by blob (object format and oid), so a file that is unchanged between commits, or present at two paths, is extracted once. `store` replaces any earlier rows for the blob and sets `blob.facts = 1`, all inside a SAVEPOINT so it works both alone and inside the index run's transaction. A blob whose language has no extractor is marked by `facts::index_blob`, so it is not offered again.

### core/src/symbols/graph.rs · link
- refers-to: core/src/symbols/graph.rs :: link; core/src/symbols/graph.rs :: build
- kind: rationale
- logged: v0.4.0 · 2026-10-30

The edge table is derived, so it is rebuilt for the whole corpus every time: delete, then insert, in one SAVEPOINT. Only files with role source, data or doc take part, so mirrors, generated and vendored copies never create edges. Resolution order for `call` and `type`: the file the name was imported from, any file the file imports, the same file, then a corpus-wide match, always within the same language family (ts, tsx and js are one family). The first stage that has candidates decides: one candidate gives an edge, several are counted as ambiguous and skipped, none moves to the next stage. Names with no candidate are counted as unresolved; most of those are library and built-in calls (`map`, `push`, `Promise`). `invoke` edges go to a unique `tauri_command` by name, and `css_var` edges go to the earliest definition by path and line. A reference never links to the unit that contains it. Python modules are resolved against the importing file's own ancestor directories nearest first, so `from app.routers import x` finds `backend/app/routers/x.py` from inside backend/. Rust `use` paths are recorded as imports but not resolved to files in this phase.

### core/src/symbols/graph.rs · LinkStats
- refers-to: core/src/symbols/graph.rs :: LinkStats
- kind: warning-context
- logged: v0.4.0 · 2026-10-30

On formvis at HEAD the first link gave 62,547 edges, 16,175 ambiguous and 65,607 unresolved references. The ambiguous count is large because the walk is by name, not by receiver type: a bare `render()` or `get()` cannot be tied to one class. Corpus-wide matches on very common method names can also produce a wrong edge when exactly one unit in the corpus has that name and the call was really to a library method. Treat `call` edges as leads, `import`, `invoke` and `css_var` edges as reliable. `--accent-color` has 147 edges (145 from CSS, 2 from TypeScript strings); `revealApp`'s `invoke app_ready` resolves to `src-tauri/src/reveal.rs#app_ready`.

## migrations/002_symbols.sql

### core/src/migrations/002_symbols.sql · unit, unit_ref, unit_import, edge
- refers-to: core/src/migrations/002_symbols.sql :: unit; core/src/migrations/002_symbols.sql :: edge
- kind: explainer
- logged: v0.4.0 · 2026-10-30

`unit`, `unit_ref` and `unit_import` hold per-blob facts and have no corpus column, so they are shared by every corpus that contains the blob. `edge` is per corpus and is rebuilt by `link`. The migration also adds `blob.facts` (0 until extracted), which the index run uses to find blobs that still need extraction. SQLite cannot add a column with a non-constant default; the constant 0 is fine, and `ALTER TABLE ... ADD COLUMN` is the only form used. No foreign keys from the fact tables to blob, so a re-extract can replace rows without cascade effects.

## dedup.rs

### dedup.rs · module
- refers-to: core/src/dedup.rs :: (module)
- kind: explainer
- logged: v0.4.0 · 2026-10-30

Phase S4. The module turns plain spans (byte ranges inside one blob, supplied by the symbol extractor through `facts::index_blob` but never typed by it) into prints, stores the prints per blob, and later links them across all corpora. It does not import any symbols type: the only inputs are `Span`, the file text and the language name. The tiers follow the research dossier A03: T1 same raw text (Type 1), T2 same normalised token stream (Type 2), T3 near duplicate by MinHash with banding and winnowing confirmation (Type 3). Type 4 (semantic) is out of scope here and stays advisory in a later phase.

### dedup.rs · tokenize
- refers-to: core/src/dedup.rs :: tokenize, Token
- kind: rationale
- logged: v0.4.0 · 2026-10-30

A small hand-written lexer, not a parser, because T2 and T3 only need a stable token stream, not a tree. Four families: TS and JS (also `tsx`, `jsx`), Python, Rust, CSS (also `scss`, `less`); anything else is lexed as TS/JS. Whitespace and comments are dropped. Identifiers that are not keywords become `$id`, numbers `$num`, strings, template literals, regex literals, Rust char literals and raw strings `$str`; keywords and punctuation keep their text. Multi character operators are matched greedily from two short lists so that `=>` or `===` count as one token, which keeps token counts close to what a real lexer reports.

The A03 text asks for alpha-renaming (first-occurrence indices) for T2 and for keeping imported names unrenamed. This build uses the simpler blanket `$id` rule: it matches every pair that alpha-renaming matches, plus pairs where two different locals are merged (`a + a` against `a + b`), so T2 here is slightly more generous than A03. The cost is a few false T2 links; the benefit is that T2 needs no scope analysis and works for CSS, where scope does not exist. If T2 proves noisy in the form* corpus the lexer is the single place to add first-occurrence indices.

Edge cases that were deliberately handled: JS regex literals are told from division by the previous token (a value token or `)` `]` `}` means division), because an unlexed `/['"]/` would otherwise swallow the rest of the file as a string; Rust lifetimes (`'a`) are told from char literals (`'a'`) by looking for the closing quote; Python string prefixes (`rb`, `f`) are dropped so the string token is the same with or without them; CSS identifiers may contain `-`, colours `#abc` and `#aabbcc` become `$num` and other `#name` become `$id`. Quoted strings stop at a newline (except template literals and Python triple quotes) so one stray apostrophe cannot eat a file.

### dedup.rs · hash family
- refers-to: core/src/dedup.rs :: SCHEME, mix, token_hash, shingle_hashes, permutations, minhash
- kind: explainer
- logged: v0.4.0 · 2026-10-30

The MinHash hash family is fixed and versioned by `SCHEME` (1), stored on every print row. A stored signature is only comparable with signatures of the same scheme; changing any constant below means bumping `SCHEME` and re-fingerprinting every blob (the prints are derived, so a rebuild is safe; bumping `facts::VERSION` does it).

- Token hash: FNV-1a 64 over the normalised token text, finished with the splitmix64 finaliser `mix`.
- Shingle hash: the 5 token hashes of a 5-gram folded as `h = mix(rotl(h, 7) ^ token_hash)` starting from the seed `0x6A09E667F3BCC908`.
- Permutations: 128 pairs `(a_i, b_i)`, `a_i = mix(seed + (2i+1) * 0x9E3779B97F4A7C15) | 1` (odd, so the map is a bijection on 64 bits) and `b_i = mix(seed + (2i+2) * 0x9E3779B97F4A7C15)`.
- Signature value i: the minimum over all shingles x of `((a_i * x + b_i) mod 2^64) >> 32`, taken as a u32.

This is a multiply-add universal family, the standard cheap approximation of random permutations for MinHash; the estimated Jaccard of two units is the fraction of the 128 positions where their signatures agree. No external hash crate is used for MinHash because no maintained Rust MinHash or LSH crate exists (dossier D01); blake3 is used only for the two identity hashes.

### dedup.rs · thresholds
- refers-to: core/src/dedup.rs :: SHINGLE, PERMUTATIONS, BANDS, ROWS, JACCARD_MIN, MIN_TOKENS, MIN_GROUP_TOKENS, BUCKET_CAP, EXPAND_CAP
- kind: rationale
- logged: v0.4.0 · 2026-10-30

All thresholds come from dossier A03, except `MIN_GROUP_TOKENS`.

- Shingle size 5 and Jaccard 0.70: the StarCoder near-dedup choice (A03 sources 25 and 26), and the BigCloneBench floor for strong Type 3 (A03 source 13: 70 to 90 percent). Below 0.70 a syntactic method does not find clones reliably, so nothing below it is recorded as a clone.
- 128 permutations, 16 bands of 8 rows: a pair with Jaccard s is a candidate with probability `1 - (1 - s^8)^16`, an S-curve whose midpoint is `(1/16)^(1/8)`, about 0.71, matching the 0.70 verification threshold. Candidates are then verified on the full signature, because banding gives false positives and negatives (A03 source 5).
- Winnowing k = 5 and guarantee t = 30 tokens, so the window is `w = t - k + 1 = 26` shingles. Any shared run of 30 or more tokens shares at least one fingerprint (Schleimer, Wilkerson, Aiken, A03 source 10); about 7.4 percent of shingles are kept on average. The minimum of each window is chosen, ties to the rightmost, and the stored list is sorted and de-duplicated.
- 30 tokens is also the floor for T3: spans under 30 tokens get only the two hashes (empty `minhash` and `winnow`). A03 suggests 50 tokens or 6 lines for the BigCloneBench floor; 30 matches the winnowing guarantee, and units between 30 and 50 tokens can be filtered later at query time.
- `MIN_GROUP_TOKENS` (12): units under 12 tokens take no part in T1 and T2 groups. Without it, one-line constants (`ROOT = ...`) and tiny getters formed hundreds of exact groups across repositories (formMCP against formskills went from 2,476 output lines to 715). Chosen on 2026-10-30 from that run, not from a paper.
- Confirmation by shared winnow fingerprints: a verified pair must share at least one fingerprint and at least a quarter of the smaller side's fingerprints. This removes pairs whose signatures collide on boilerplate shingles but share no real run. The quarter is a first choice, not a calibrated value; A03 says all thresholds must be validated on form* code.
- `BUCKET_CAP` (300): a band bucket with more members than this is boilerplate (every unit shares that band) and is skipped, so candidate generation stays linear in practice. `EXPAND_CAP` (64): a near match between two classes that each occur in many places would multiply into thousands of edges; at most 64 occurrence pairs are recorded per class pair.
- Units of kind `selector`, `custom_property` and `element_id` are skipped for T3 only; they are too small for 5-gram similarity to mean anything. They still take part in T1 and T2 when they reach 12 tokens.

### dedup.rs · fingerprint
- refers-to: core/src/dedup.rs :: fingerprint, normalise_lines
- kind: explainer
- logged: v0.4.0 · 2026-10-30

One `Print` per span, `idx` being the position in the span list, which equals the `idx` of the symbol unit because `facts::index_blob` builds the spans from the units in order. `raw_hash` is blake3 over the span bytes with trailing spaces, tabs and carriage returns trimmed on every line and line endings turned into `\n` (Type 1: same text up to layout at line ends). `norm_hash` is blake3 over the normalised tokens each followed by byte 0x1F (Type 2). Offsets are clamped to the text, and bytes that are not valid UTF-8 are replaced, so a bad range can never panic. Both hashes are 64 hex characters; blake3 1.8.7 is used under its Apache-2.0 option (dossier D01), so no CC0 ruling is needed.

### dedup.rs · store
- refers-to: core/src/dedup.rs :: store, write_prints
- kind: rationale
- logged: v0.4.0 · 2026-10-30

Rows are keyed by (object_format, oid, idx), the same key as the blob, so a blob seen in many corpora or paths is fingerprinted once. Earlier rows for the blob are deleted first, which makes a re-run idempotent. The work runs inside a SAVEPOINT, not a transaction, so it nests safely inside the indexer's own transaction; on error it rolls back to the savepoint and the caller's transaction is left usable. Signatures are stored as little-endian BLOBs: u32 for MinHash, u64 for winnowing.

### dedup.rs · link
- refers-to: core/src/dedup.rs :: link, group_edges, LinkStats
- kind: explainer
- logged: v0.4.0 · 2026-10-30

`link` rebuilds `clone_group`, `clone_member` and `clone_pair` completely, inside a SAVEPOINT (`dedup_link`) so it runs within the index run's transaction (the first version opened its own transaction, which SQLite refused inside the index run), from `file` (role `source` only) joined to `unit` and `unit_print` through the corpus object format. It is deterministic: groups and candidates are sorted before they are written. It never builds anything from files that have no units. Every index run of any corpus calls it, which costs about 0.9 s with formvis, formMCP and formskills indexed.

Groups: T1 is every set of two or more occurrences with the same `raw_hash`. T2 is every set with the same `norm_hash` that holds at least two different `raw_hash` values; its members include all occurrences, so a T2 group can contain exact copies as well. Units under `MIN_GROUP_TOKENS` are ignored.

Edges: a group of N members would make N(N-1)/2 pairs, which explodes on a hot snippet copied 40 times. `group_edges` records instead one edge from the first member of each corpus to every other member of that corpus, plus one edge between the first members of every two corpora present. Every member is therefore reachable, and every pair of corpora that share a clone has a direct edge, in at most N plus c(c-1)/2 edges. In T2 the edges join one representative per distinct raw hash. No edge joins a unit to itself or to an overlapping range in the same file (`same_place`), and each pair is recorded once, in index order, so (a, b) and (b, a) never both appear.

T3: units are first collapsed into classes by `norm_hash` (a class has one signature), banding runs over classes, candidate pairs are verified (Jaccard on signatures at least 0.70, then shared winnow fingerprints), and each verified class pair is expanded into occurrence pairs up to the cap. A T3 edge is never made between members of the same class, because those are already T1 or T2. The score is the estimated Jaccard.

### dedup.rs · what T1 to T3 do not find
- refers-to: core/src/dedup.rs :: link
- kind: warning-context
- logged: v0.4.0 · 2026-10-30

On formvis the B06 cluster `filter-preview.ts` against `gallery-view.ts` is found (T1 `paint`, `pan`, `grab`). The six colour-row twins B06 lists (`colorRow` and `colourRow` in controls.ts, page-prefs.ts, app-dialogs.ts, stroke-path-dialog.ts, content-fill-dialog.ts, artboard-dialog.ts) are not: they do the same job with different bodies (a function, a private method with other parameters, a local variable), so they are below Jaccard 0.70. They are "same purpose" duplicates, which is what S5's slots and a related band (A03's 0.5 to 0.7) or same-name sibling report are for; recorded in DEV_PLAN as carried into S5.

### dedup.rs · drift
- refers-to: core/src/dedup.rs :: drift, DriftRow, state_rank
- kind: explainer
- logged: v0.4.0 · 2026-10-30

The four states are the ones in dossier A03 section 8, without the direction analysis (that needs a recorded copy base, which S5's lockfile brings). For each unit of corpus A the candidates in corpus B are the units with the same qualified name; when there is none (or the name is empty) they are the units in the same relative path. The best candidate is the one with the lowest state rank, then the highest score, then the same path. `identical`: same raw hash. `renamed`: same norm hash. `drifted`: estimated Jaccard at least 0.70. `diverged`: same name, anything else (score is the estimate, 0 under 30 tokens). A diverged result found only through the same-path fallback is dropped, because two unrelated units that happen to share a file are not evidence of drift. Units of B that have no counterpart in A are not listed: drift is read from A to B. formMCP to formskills: 127 identical, 70 renamed, 27 drifted, 49 diverged.

A caveat for readers of the output: because T2 treats every identifier as `$id`, `renamed` also reports two different functions of the same shape in the same file when the path fallback pairs them. The score column cannot tell those from a real rename; the names can.

## migrations/003_dedup.sql

### core/src/migrations/003_dedup.sql · unit_print, clone_group, clone_member, clone_pair
- refers-to: core/src/migrations/003_dedup.sql :: unit_print; core/src/migrations/003_dedup.sql :: clone_pair
- kind: explainer
- logged: v0.4.0 · 2026-10-30

`unit_print` is per blob, keyed like `unit` by (object_format, oid, idx) with indexes on `raw_hash` and `norm_hash`; `scheme` records the MinHash family. The three clone tables are derived and rebuilt by `dedup::link` across all corpora. The migration only creates tables; it reads nothing from `unit`, so it does not depend on 002 having data.

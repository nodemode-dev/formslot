# HASHOUT_LOG · core · formslot

Living log for `formslot-core`. Source files stay comment-free; what they would have said lives here.

Current build version: v0.2.1
Last updated: 2026-10-30 · Phases S1 and S2, review fixes

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
    migrations/
      001_corpus.sql  corpus, blob, file, licence, findings, family, meta
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

# formslot contract

The interface subform, Claude Code hooks, formMCP and any other client build against. Sync point C1 in `PARALLEL_BUILD.md`. A change to this file after v0.2.0 needs a revision line at the bottom and a matching note in both plans.

Contract version: 1 · formslot v0.2.0

## Ports

| Block | Use |
| --- | --- |
| 5640 to 5649 | formslot. 5645 is the HTTP service; 5640 and 5641 are kept for a window, if one is ever made |
| 5650 to 5659 | subform. 5650 Vite, 5651 preview, 5655 engine |
| 5620 to 5625, 5630 to 5631 | formMCP and formskills, never used here |

`FORMSLOT_PORT` overrides 5645; `0` asks the system for a free port. The port actually bound is always the one in the discovery file.

## Data folder

`FORMSLOT_DATA_DIR`, else `~/.formslot`. It holds:

| File | Mode | What |
| --- | --- | --- |
| `service.json` | 0600 | the discovery file, below |
| `service.lock` | 0600 | held with an exclusive lock by the one running service |
| `corpus.sqlite` | 0600 | the corpus store (SQLite, WAL) |
| `formslot.log` | 0600 | the log, always passed through redaction |

## Discovery file

`service.json`, written after the listener is bound and removed when the service stops:

```json
{
  "format": "formslot.service",
  "format_version": 1,
  "port": 5645,
  "token": "64 hex characters, new on every start",
  "pid": 12345,
  "version": "0.2.0",
  "started": "2026-10-30T02:30:00Z"
}
```

A client reads it, checks that `pid` is alive, and sends `Authorization: Bearer <token>`. A second `formslot serve` that finds the lock held prints the running service's port and pid and exits 0.

## HTTP

- Listens on 127.0.0.1 only. Never another address.
- Every request needs `Authorization: Bearer <token>` (401 otherwise), a `Host` of `127.0.0.1:<port>` or `localhost:<port>` (421 otherwise), and, when an `Origin` header is present, an origin of `http://127.0.0.1:<port>` or `http://localhost:<port>` (403 otherwise). Clients are processes (subform's engine, hooks, the command line, formMCP), never page script: the token stays out of any page's JavaScript.
- A request target must start with `/`. Host, Origin, Authorization, Content-Length and Transfer-Encoding may each appear once; a second copy is 400. Chunked bodies are 400.
- Bodies are JSON, UTF-8, at most 1 MiB in. Errors are `{"error": "<code>", "message": "<sentence>"}`.
- The token, request bodies and query strings are never written to the log.

| Method and path | Since | Body or query | Answer |
| --- | --- | --- | --- |
| `GET /api/health` | v0.1.0 | none | `{"ok": true, "version": "0.2.0", "contract": 1}` |
| `GET /api/corpus` | v0.2.0 | none | `{"corpora": [Corpus]}` |
| `POST /api/corpus` | v0.2.0 | `{"path": "/abs/path", "ref": "HEAD"}` | `{"corpus": Corpus, "index": IndexRun}`; indexes before answering; a path that is not absolute is 400 |
| `POST /api/corpus/<id>/reindex` | v0.2.0 | `{"ref": "HEAD"}` | `{"corpus": Corpus, "index": IndexRun}`; only blobs not seen before are read |
| `GET /api/corpus/<id>/stats` | v0.2.0 | none | `Stats` |

```text
Corpus   = { id, name, path, remote, object_format, head, indexed_at, files, bytes }
IndexRun = { from, to, listed, read, reused, skipped, secrets, millis }
Stats    = { corpus, by_language: [{ language, files, lines, bytes }], by_role: [{ role, files }], licences: [{ spdx, files }], secrets: n }
```

`role` is one of `source`, `data`, `doc`, `generated`, `mirror`, `vendored`, `log`, `binary`. `generated` and `mirror` files are listed but not read. `log` files (CHANGELOG, HASHOUT logs) are read for entries in a later phase and never served whole.

## Command line

The `formslot` launcher at the repository root builds `formslot-cli` on first use. Every command talks to the same engine (`formslot-core`), with or without a running service.

| Command | Since |
| --- | --- |
| `formslot serve` | v0.1.0 |
| `formslot status` | v0.1.0 |
| `formslot doctor` | v0.1.0 |
| `formslot add <path> [--ref <rev>]` | v0.2.0 |
| `formslot reindex <name> [--ref <rev>]` | v0.2.0 |
| `formslot stats <name> [--json]` | v0.2.0 |
| `formslot files <name> [--role <role>] [--language <lang>]` | v0.2.0 |
| `formslot family --family-json <path> [--check]` | v0.2.0 |
| `formslot symbols <name> [--lang L] [--kind K] [--exported] [--count]` | v0.4.0 |
| `formslot show <name> <path>#<qualified>` | v0.4.0 |
| `formslot refs <name> <symbol>` | v0.4.0 |
| `formslot dupes <name>` and `formslot dupes --across <a> <b> [--limit N]` | v0.4.0 |
| `formslot drift <a> <b> [--all]` | v0.4.0 |

Unit kinds (v0.4.0): `function`, `method`, `class`, `interface`, `type`, `enum`, `namespace`, `const` (an exported TypeScript constant), `constant` (an UPPER_CASE module-level Python name or a Rust const), `struct`, `trait`, `static`, `mod`, `tauri_command`, `custom_property`, `selector`, `element_id`, `script`, `style`. Edge kinds: `import`, `call`, `type`, `invoke`, `css_var`. Clone tiers: `T1` same text, `T2` same normalised tokens, `T3` near duplicate (estimated Jaccard at least 0.70). Drift states: `identical`, `renamed`, `drifted`, `diverged`. A unit's stable id is `formslot <repo> <path>#<qualified>~<sig8>`.

## Later, fixed now so clients can plan

These land in later phases; names and shapes are fixed here and change only with a revision line.

- MCP tools (S8): `slot_find`, `slot_get`, `slot_usages`, `slot_similar`, `slot_context_pack`, `slot_conventions`, `slot_status` (read-only), `slot_reindex`. Served at `POST /mcp` on the same port with the same checks, and over stdio by `formslot mcp`.
- Write check (S9): `POST /api/check` with `{"tool": "Write" | "Edit", "path": "...", "content": "...", "session": "..."}` answers within one second with `{"action": "allow" | "warn" | "deny", "message": "...", "slots": [SlotRef]}`. `warn` is the default for duplicates; `deny` is only for the owner's declined features.
- Excerpt envelope: every piece of indexed text sent to a model is `{"kind": "formslot.excerpt", "source": {"repo", "path", "commit", "lines"}, "slot": "id or null", "licence": "SPDX or unknown", "copy": "allowed | notice | no_copy", "text": "..."}` and is data, never instructions.

## Revisions

- 1 · 2026-10-30 · first version, written in Phase S1
- 1 · 2026-10-30 · v0.2.1, compatible tightening after review: duplicate checked headers, targets without a leading slash and relative corpus paths are 400; `Corpus.remote` never carries credentials
- 1 · 2026-10-30 · v0.4.0, additive: the symbol, refs, dupes and drift commands, with their unit kinds, edge kinds, tiers, drift states and the stable id form

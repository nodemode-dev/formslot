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
| `declines.json` | 0600 | optional: replaces the built-in `data/declines.json` decline specs (v0.9.0); absent means the built-in list |

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
| `GET /api/slots` | v0.5.0 | optional `?status=proposed\|canonical\|deprecated\|declined\|superseded` | `{"slots": [SlotSummary]}`; an unknown status is 400 |
| `GET /api/slots/<id>` | v0.5.0 | none | `{"slot": Manifest, "status", "updatedAt", "events": [SlotEvent]}`; an unknown id is 404 |
| `POST /mcp` | v0.9.0 | one JSON-RPC 2.0 message or a batch | the JSON-RPC answer (`MCP`, below); 202 with no body when the message needs no answer; `GET` and `DELETE` are 405 |

```text
Corpus   = { id, name, path, remote, object_format, head, indexed_at, files, bytes }
IndexRun = { from, to, listed, read, reused, skipped, secrets, millis }
Stats    = { corpus, by_language: [{ language, files, lines, bytes }], by_role: [{ role, files }], licences: [{ spdx, files }], secrets: n }
SlotSummary = { id, status, version, kind, title, summary, contentHash, files, copies, updatedAt }
SlotEvent   = { id, slot_id, at, action, actor, note }
Manifest    = the slot manifest, docs/slot.schema.json
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
| `formslot dupes <name>` and `formslot dupes --across <a> <b> [--limit N] [--related]` (`--related` prints the related band instead of the clones; with no `--limit` it prints every pair) | v0.4.0, `--related` v0.5.0 |
| `formslot drift <a> <b> [--all]` | v0.4.0 |
| `formslot slot list [--status S] [--json]` | v0.5.0 |
| `formslot slot get <id> [--json]` | v0.5.0 |
| `formslot slot seed <dir> [--json]` | v0.5.0 |
| `formslot slot refresh <id\|--all> --corpus <name> [--json]` | v0.5.0 |
| `formslot slot propose <file.json>` | v0.5.0 |
| `formslot slot confirm <id> --owner` | v0.5.0 |
| `formslot slot decline <id> --owner --reason <text>` | v0.5.0 |
| `formslot slot plan --repo <path> <id>... [--force] [--json]` | v0.5.0 |
| `formslot slot apply --repo <path> <id>... [--dry-run] [--force] [--json]` | v0.5.0 |
| `formslot slot drift --repo <path> [--infer] [--check] [--json]` | v0.5.0 |
| `formslot memory ingest <corpus> [--json]` | v0.7.0 |
| `formslot memory list [--kind K] [--status S] [--corpus C] [--limit N] [--count] [--json]` | v0.7.0 |
| `formslot memory declines [--check <corpus>] [--json]` | v0.9.0 |
| `formslot recall <query> [--anchor path#symbol] [--kind K] [--corpus C] [--limit N] [--json]` | v0.7.0 |
| `formslot remember --propose <text> [--anchor path#symbol] [--kind decision\|how-to] [--corpus C] [--json]` | v0.7.0 |
| `formslot search <query> [--corpus C] [--limit N] [--json]` | v0.7.0 |
| `formslot search rebuild [--corpus C]` | v0.7.0 |
| `formslot map <corpus> [--tokens N]` and `formslot map <corpus> --formmap [--tokens N]` | v0.7.0 |
| `formslot pack <task> [--corpus C] [--tokens N] [--tier T1\|T4\|T16] [--count local\|remote] [--json]` | v0.7.0 |
| `formslot mcp` | v0.9.0 |
| `formslot check [--mode warn\|deny\|off]` | v0.9.0 |
| `formslot hook <SessionStart\|UserPromptSubmit\|PostToolUse\|PreCompact\|SubagentStart\|Stop>` | v0.9.0 |
| `formslot install <repo> [--mode warn\|deny] [--enable] [--dry-run]` | v0.9.0 |

Unit kinds (v0.4.0): `function`, `method`, `class`, `interface`, `type`, `enum`, `namespace`, `const` (an exported TypeScript constant), `constant` (an UPPER_CASE module-level Python name or a Rust const), `struct`, `trait`, `static`, `mod`, `tauri_command`, `custom_property`, `selector`, `element_id`, `script`, `style`. Edge kinds: `import`, `call`, `type`, `invoke`, `css_var`. Clone tiers: `T1` same text, `T2` same normalised tokens, `T3` near duplicate (estimated Jaccard at least 0.70), `related` (v0.5.0, stored apart from the clone tiers in `related_pair`: reason `similar`, estimated Jaccard from 0.40 to below 0.70 within one unit kind family, or reason `same_name`, the same normalised name in different files). Drift states: `identical`, `renamed`, `drifted`, `diverged`. A unit's stable id is `formslot <repo> <path>#<qualified>~<sig8>`.

## Slots (v0.5.0)

A slot manifest follows `docs/slot.schema.json` (JSON Schema 2020-12). The id is lowercase dotted kebab (`ui.control-rows`, `tokens.studio-dark`) and stable forever. `status` is `proposed`, `canonical`, `deprecated`, `declined` or `superseded`; agents and seeds only ever produce `proposed`, and only `formslot slot confirm <id> --owner` reaches `canonical`. Nothing is deleted: the store refuses `DELETE` on slots, their files and their events, and the event log refuses `UPDATE`. The 26 first-load candidates ship as `proposed` manifests in `data/slots/` (the 25 seeded from B06 and `ui.prefs-dialog`, v0.9.0).

- `files[].sha256` is the SHA-256 of the LF-normalised bytes; `lines` counts lines of those bytes; `contentHash` is the SHA-256 of the files sorted by path, each as `path`, a NUL byte, `sha256`, a newline. An empty string means not computed yet; `slot refresh` fills them.
- `copies[]` is derived, never hand-written: `{repo, path, qualified, lines: [start, end], relation, score}` with relation `clone-T1`, `clone-T2`, `clone-T3`, `related-similar` or `related-same_name`: any unit linked in `clone_member`, `clone_pair` or `related_pair` to a unit inside one of the slot's files in its origin corpus. Copies go stale after a reindex until `slot refresh` runs.
- `slots.lock.json` in a consumer repository, written only by `slot apply`: `{schemaVersion: 1, slots: [{id, version, originCommit, installedAt, contentHash, files: [{path, originPath, originSha256, installedSha256, port}], pinned}]}`. An `originSha256` of `""` means the installed file differed from the origin when recorded and its base release is unknown; an `installedSha256` of `""` means the file was not in the repository.
- Drift per file: local change is installed-now against `installedSha256`; origin change is the slot's current `sha256` against `originSha256`. States `clean`, `local`, `behind` (origin only), `diverged` (both), `missing`. `drift --infer` with no lockfile matches files by name and reports `identical` or `drifted`.
- formslot only reads a consumer repository and writes its one `slots.lock.json`; it never copies or edits a consumer's source.

## Memory (v0.7.0)

A memory record has `kind`, `authority`, `status`, `title`, `text`, an optional `anchor` (`path#symbol`, or a path), `corpus_id`, `source` (`path:line`, or `proposal`), comma-separated lowercase `triggers`, `content_hash`, `valid_from`, `valid_to` and `superseded_by`.

- Kinds: `rule`, `decision`, `how-to`, `declined`, `hashout`, `tombstone` (a HASHOUT entry about removed code).
- Authority: `owner` (CLAUDE.md and AGENTS.md rules, NOTICE conventions, declines), `derived` (HASHOUT entries, BUILD_NOTES decisions, dev-wise how-tos), `agent` (proposals).
- Status: `active`, `stale` (the source line or the anchored symbol is gone; review it), `superseded` (a newer record replaced it; `valid_to` and `superseded_by` are set), `proposed` (an agent proposal, not yet reviewed).
- Nothing is deleted: the store refuses `DELETE` on records and events, `UPDATE` of the event log, `UPDATE` of text, title, kind or authority on an owner record, and promoting a record to owner after it is stored. An owner record changes only by a new record that supersedes it.
- Ingest is idempotent by content hash and source and runs after an index run that changed something; a changed record gets a new record and the old one is superseded; a vanished source line marks its record stale; anchors are checked against the `unit` table.
- `recall` ranks by whole-word term score over title, text, anchor and triggers (words are compared as sets of light stems, so plural forms match, never as substrings; terms shorter than 2 characters and stop words are dropped, and a query left with no term returns nothing; a declined record whose trigger phrase is in the query ranks first) and ends with a token estimate (characters / 4, rounded up). With `--anchor` the answer is held to 2,000 tokens, longest texts truncated first and marked. `proposed` records are left out of `recall` (they are listed by `memory list --status proposed`), and `--limit` is held to 50.
- Title and text are passed through the secret redaction before they are stored, so `recall`, `memory list`, FORMMAP and packs never carry a secret-shaped value; `content_hash` is taken over the source text, so redaction does not make an unchanged line look changed. An owner record's text, title, kind, authority, anchor, triggers, hash, corpus and dates are fixed by triggers; its status moves only between `active`, `stale` and `superseded`, and `superseded` needs `superseded_by`.
- `remember --propose` stores a `proposed` agent record and prints NOOP, MERGE, SUPERSEDE or ADD against what exists; it never deletes and never edits an owner record. The write is one transaction, and a NOOP adds a `proposed` event to the record that already holds the text instead of a new row (the answer then has `created: false` and `id` is that record).
- The declined records come from data, not from code: `data/declines.json` (embedded in the binary; a file of the same shape at `<data folder>/declines.json` replaces it) lists each decline as `{title, corpus, find, pick, triggers}`: `find` are lowercase phrases that must all be on one line of CLAUDE.md or AGENTS.md, `pick` selects the sentences kept, `corpus` is the repository the line belongs to. `formslot memory declines` lists the specs and where they come from. `formslot memory declines --check <corpus>` reads that corpus the way ingest does and prints each spec with `path:line` where it matched (`ok`), `skip` for a spec that belongs to another corpus, or `FAIL` when a spec for this corpus no longer finds its line; it exits 1 on any FAIL. A record whose source line vanished while it was stale for its anchor stays stale when the symbol returns.

## Search, map and pack (v0.7.0)

- `search` fuses five ranked lists with Reciprocal Rank Fusion (k = 60): bm25 over unit name, split name, signature and body (`unit_fts`), a trigram index over name, qualified name and path (`unit_tri`), graph neighbours of the best hits over `edge`, memory records (`memory_fts`, active only; derived `hashout` and `tombstone` records count for less) and slots (canonical and proposed). Units in test paths are halved. A hit is `{kind: "unit" | "memory" | "slot", id, corpus, path, lines: [start, end] | null, score, why: [..]}`; a unit id is the stable id, a memory id is the record id, a slot id is the slot id (its `path` is the first file, `corpus` the origin repo).
- The unit indexes (`search_unit`, `unit_fts`, `unit_tri`) are written by the index run and by the first search, map or pack on a corpus that has none; `search rebuild` drops and rebuilds them and the memory index. `memory_fts` follows `memory_record` through triggers.
- `map` ranks units with a PageRank walk over the `edge` table (damping 0.85, 40 iterations, kinds weighted `call`, `type`, `invoke` 1.0, `import` 0.5, `css_var` 0.4, repeat edges by square root) and prints the largest prefix of the ranking that fits `--tokens` (default 1024), grouped by file, one line per file. `--formmap` writes the FORMMAP.md form: header, source file counts, owner rules, shared slots and the map, at most 148 lines and `--tokens` (default 4096). When the header would take more than 60 percent of `--tokens`, shared slots and then owner rules are cut from the end until it fits; when even the bare header (title, generated line, source counts) does not fit, the command exits 1 and names the minimum. The output has no timestamp.
- Token estimates are local: `ceil(bytes / 3.6)` for code and envelopes, `ceil(bytes / 4)` for prose. `pack --count remote` calls the Messages `count_tokens` endpoint (`https://api.anthropic.com/v1/messages/count_tokens`, model `claude-sonnet-5-5` or `FORMSLOT_COUNT_MODEL`) with the key from the system keyring entry `formslot/anthropic`, else `ANTHROPIC_API_KEY`; it exits 1 when neither exists, prints `remote count (Messages count_tokens): N tokens` on stderr (`remoteTokens` with `--json`), and on a network or API failure says so and keeps the local estimate. It is the only command that makes a network call.
- `pack` answers with the prompt on stdout (the summary on stderr) or, with `--json`:

```json
{
  "format": "formslot.pack",
  "format_version": 1,
  "task": "...",
  "corpus": "formvis",
  "commit": "40 hex",
  "tier": "T1 | T4 | T16",
  "budget": 4000,
  "estimator": "ceil(bytes / 3.6) for code and envelopes, ceil(bytes / 4) for prose",
  "p0_sha256": "64 hex",
  "tokens": {"p0": n, "p1": n, "p2": n, "sent": n},
  "parts": {"p0": Part, "p1": Part, "p2": Part},
  "slots": [{"id", "status", "title", "files": [path], "why"}],
  "hits": [Hit],
  "prompt": "the parts that are sent, joined",
  "millis": n
}
Part = {"included": bool, "tokens": n, "sha256": "64 hex", "text": "..."}
```

- `P0` is the stable prefix: the data notice, the corpus head, the owner rules (active `rule` records of authority `owner`, ordered by id, 350 tokens at most) and the repo map head (650 tokens). It carries no time and no per-call value, so it is byte-identical for one corpus head and one set of owner rules, whatever the task or tier, and `p0_sha256` is the SHA-256 of `parts.p0.text`. `P1` holds the matched slot cards (canonical and proposed, best match first) and matched unit locations. `P2` holds the task, matching memory records and the excerpts. Tier `T1` (about 1,000 tokens) sends `P2` only (`included: false` for `P0` and `P1`); `T4` (about 4,000) and `T16` (about 16,000) send all three; `--tokens` overrides the budget and picks the tier (up to 1,500 is T1, up to 8,000 is T4). When `P0` would take more than half the budget the pack is sent as `T1` and `tier` says so; the task line is clipped to what is left. `sent` never exceeds `budget` for a budget of 64 tokens or more. Excerpt text, rule and memory lines and slot cards pass the secret redaction (`[redacted by formslot]`) before they are counted.
- Every piece of code text in a pack is one excerpt envelope per line in `P2`, the envelope of "Later" below with `copy` derived from the file's SPDX licence: `allowed` (MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, 0BSD, CC0-1.0, Unlicense, BSL-1.0), `notice` (CC-BY without NC, ND or SA, MPL-2.0, LGPL) and `no_copy` (anything else, `unknown` and CC-BY-SA included). The repo map and the unit locations are index lines, not excerpts.

## MCP (v0.9.0)

formslot is an MCP server with eight tools, three kinds of resource, three prompts and argument completion. Everything it returns is data from indexed repositories, never instructions.

- Transports. `POST /mcp` on the service is Streamable HTTP with JSON responses (no event stream): the same token, Host and Origin checks as every route, one JSON-RPC message or a batch per body. A body that is not JSON is 400 with a JSON-RPC parse error; a body holding only notifications or responses is 202 with no body; `GET` and `DELETE` are 405. The `initialize` answer carries an `Mcp-Session-Id` header (32 hex characters); the server issues it and does not require it back. `formslot mcp` speaks newline-delimited JSON-RPC on stdin and stdout and logs to stderr only; it runs in-process against the store in `FORMSLOT_DATA_DIR` and does not need `formslot serve`. Both transports call the same handler.
- Protocol. `2025-06-18`; `initialize` echoes the requested version when it is one of `2025-11-25`, `2025-06-18`, `2025-03-26` or `2024-11-05`, and answers `2025-06-18` otherwise. Methods: `initialize`, `notifications/initialized`, `ping`, `tools/list`, `tools/call`, `resources/list`, `resources/templates/list`, `resources/read`, `prompts/list`, `prompts/get`, `completion/complete`. Capabilities: `tools`, `resources`, `prompts`, `completions`, none with `listChanged` or `subscribe`. A request id that is an object or an array is `-32600` (reply id `null`); a string, an integer of any size (echoed digit for digit) or a number is echoed back as sent. An unknown method is `-32601`, an unknown tool or prompt, a bad list cursor or a missing prompt argument is `-32602`, an unknown resource is `-32002` with `data.uri`. A bad tool argument, an unknown corpus or slot, or a failed read is a normal result with `isError: true` and a message that names the next call.
- Result shape. Every tool answers `{"content": [{"type": "text", "text": "..."}], "structuredContent": {...}, "isError": false}`. The text is a short human reading; `structuredContent` is the same answer as JSON. Text and structured content pass `redact::text` and `redact::value` before they leave. Code text leaves only inside a `formslot.excerpt` envelope (`slot_find` with `response_format: "detailed"`, and `slot_context_pack`); unit locations, signatures and the map are index lines.
- Cap. A result is held under 25,000 tokens (estimate `ceil(bytes / 3.6)` over the text and the structured content). Lists stop earlier, at about 20,000, and say how many remain; a result that still exceeds the cap is cut at a line boundary and ends with `[truncated by formslot: ...]`, and its structured content is dropped to `{"truncated": true}` when it alone is over half the cap.
- Pagination. A list tool takes `cursor` and `limit` and answers `structuredContent.total`, `offset`, `returned` and `next_cursor` (`null` on the last page); the text ends with `N more <noun>; call again with the same arguments and cursor "<cursor>" to continue.` A cursor is opaque (hex), bound to the tool and its arguments, and refused with an `isError` result when it belongs to another call. `resources/list` pages with `nextCursor` (50 per page).

| Tool | Arguments | Result |
| --- | --- | --- |
| `slot_find` | `query` (required), `corpus`, `limit` (1 to 50, 10), `response_format` (`concise` or `detailed`), `cursor` | fused search hits, slots first, then units and memory, at most 200: `hits: [{kind, id, corpus, path, lines, title?, status?, summary?, unit_kind?, qualified?}]`; detailed adds `score`, `why` and, for the first five unit hits of a page, `excerpt` (an excerpt envelope, 40 lines at most) |
| `slot_get` | `id` (required), `response_format`, `cursor` | `{id, status, updatedAt, title, summary, version, contentHash, files, events, copies: [Copy]}` with `copies` paged 25 at a time; detailed adds `manifest` and the long description |
| `slot_usages` | `target` (required: a symbol name, `path#symbol` or a slot id), `corpus`, `limit` (1 to 100, 25), `cursor` | `usages: [...]` and `mode` (`symbol` or `slot`); a reference is `{type: "reference", repo, path, qualified, line, kind, to: {path, qualified}}`; for a slot, references from outside its files come first, then `{type: "copy", ...Copy}` |
| `slot_similar` | `target` (required: `path#symbol`, a stable unit id or a slot id), `corpus`, `limit`, `cursor` | `similar: [Copy]`, closest first (`clone-T1`, `clone-T2`, `clone-T3`, `related-similar`, `related-same_name`); computed live, not from the stored `copies[]` |
| `slot_context_pack` | `task` (required), `corpus` (required when more than one is indexed), `tokens` (64 to 16000, 4000), `tier` (`T1`, `T4`, `T16`) | content block 1 is the pack prompt, block 2 a one-paragraph summary; `structuredContent` is the pack JSON of `Search, map and pack` without the part texts, plus `millis` |
| `slot_conventions` | `corpus`, `topic`, `limit` (1 to 50, 20), `response_format`, `cursor` | `records: [{id, kind, authority, status, title, text, anchor, source, corpus, score}]`: active owner records and declines, recall-ranked (at most 50 considered) |
| `slot_status` | `corpus` | `{version, contract, data_dir, service: {running, port?, pid?, started?}, corpora: [{name, head, indexed_at, files, bytes, units, edges, memory_records}], slots: [{status, count}], memory: [{kind, status, count}], clone_groups, related_pairs}` |
| `slot_reindex` | `corpus` (required), `ref` (`HEAD`) | `{corpus: Corpus, index: IndexRun}`; indexes under the service's indexing lock |

```text
Copy = { repo, path, qualified, lines: [start, end], relation, score }
```

`slot_find`, `slot_get`, `slot_usages`, `slot_similar`, `slot_context_pack`, `slot_conventions` and `slot_status` carry `readOnlyHint: true`; `slot_reindex` carries `readOnlyHint: false`, `destructiveHint: false`, `idempotentHint: true`. All eight carry `openWorldHint: false`, a title, and a JSON input schema with a description on every property. `slot_reindex` writes only formslot's own index, never a repository.

- Resources. `slot://slots/{id}` is the slot as `GET /api/slots/<id>` answers it (`application/json`); `slot://memory/{id}` is one memory record (`application/json`); `slot://map/{corpus}` is the FORMMAP text of the corpus (`text/markdown`). `resources/list` lists every slot and every corpus map; `resources/templates/list` lists the three templates; memory records are reached by template, not listed. A resource body is held under the same cap.
- Prompts. `reuse-check` (`task`): the find, follow and decide steps to run before writing code. `pack-for-task` (`task`, `corpus`): the instruction plus a 4000-token pack. `conventions` (`corpus`): the owner rules and declines of the corpus.
- Completion. `completion/complete` completes `id` of `slot://slots/{id}` (slot ids) and `slot://memory/{id}` (record ids), `corpus` of `slot://map/{corpus}`, and `corpus` of the prompts `pack-for-task` and `conventions` (`%`, `_` and `\` in the typed prefix are literal); at most 100 values, with `total` and `hasMore`.
- Logging. The log records the tool name, success and milliseconds of each call, and the client name at `initialize`; never an argument, a query or a result.

## Claude Code hooks (v0.9.0)

Hook JSON follows Claude Code's hooks reference (code.claude.com/docs/en/hooks). Every command reads one JSON object on stdin, prints at most one JSON object on stdout, and exits 0 whatever happens: an internal error, a missing store, a path outside every corpus or a timeout prints nothing and writes one line to `formslot.log`. They open the store read-only and never create it.

- `formslot check` (PreToolUse, matcher `Write|Edit|MultiEdit`). Reads `tool_name`, `tool_input.file_path`, `tool_input.content` (Write), `tool_input.new_string` and `old_string` (Edit), `tool_input.edits[]` (MultiEdit), `cwd`, `session_id`. The corpus is the one whose path holds `cwd`; a file outside it, or no corpus, prints nothing. A path is normalised lexically (`.` and `..` resolved) before it is matched to the corpus. A payload over 8 MiB is cut at 8 MiB and the tool name, path, `cwd`, session and the head of the text are still read, so declines are checked on the first 8 MiB. The whole answer is held to 900 ms.
  - Declined features: a trigger phrase of an active `declined` owner record, matched on word boundaries (camelCase and punctuation split, case folded) in text the write adds (not text already in the file or in `old_string`) or in the path of a new file, prints `{"hookSpecificOutput": {"hookEventName": "PreToolUse", "permissionDecision": "deny", "permissionDecisionReason": "..."}}`. The reason quotes the owner rule, its title, record id and source. Content of documentation files (`.md`, `.mdx`, `.txt`, `.rst`, `.adoc`, `docs/`, CLAUDE, AGENTS, NOTICE, CHANGELOG, HASHOUT_LOG, README) is not scanned.
  - Reuse: identifiers the write newly declares (top-level function, class, const, struct, trait, interface, type, enum) and, for a new file, its name are matched against canonical and proposed slots (id, title, tags, exported symbols, file names, summary) and against indexed units of the corpus. A match prints `{"hookSpecificOutput": {"hookEventName": "PreToolUse", "additionalContext": "..."}, "systemMessage": "..."}` with no `permissionDecision`, so the normal permission flow continues. The text names the slot id, status, first file with the line of the best matching symbol and, when one exists, the unit of the same purpose with its path and line. A slot or unit that holds the file being written is not reported, and one warning is given per session, path and first slot.
  - `FORMSLOT_HOOK_MODE` or `--mode`: `warn` (default; declines deny, reuse warns), `deny` (also denies when a canonical slot matches on three or more id, title, tag or export terms), `off` (prints nothing). `--mode` wins over the variable.
- `formslot hook SessionStart`: `{"hookSpecificOutput": {"hookEventName": "SessionStart", "additionalContext": "..."}}` with the FORMMAP head, the owner rule count and the declined count, at most 1,900 tokens (code estimate).
- `formslot hook UserPromptSubmit`: reads `prompt`; up to three lines (a declined feature named in the prompt, then slots, then memory records) as `additionalContext`, at most 2,800 characters; nothing for slash commands and prompts under 12 characters.
- `formslot hook PostToolUse`: records the written path for the session and starts `formslot reindex <corpus>` detached, at most once per 20 seconds per corpus; prints nothing. Declare it `"async": true`.
- `formslot hook SubagentStart`: `additionalContext` with the active owner rules and the declined features, at most 2,400 characters.
- `formslot hook PreCompact`: prints nothing; the reference gives PreCompact no way to add context. The digest is returned again by SessionStart with `source` `compact`.
- `formslot hook Stop`: when the session wrote files, the units of those files are fingerprinted from the working tree and compared with the stored clone fingerprints; T1 (same text) and T2 (same normalised tokens) clones of other files print `{"systemMessage": "..."}` once per pair, except constants that are only literal tables (arrays or objects without functions or calls); nothing is printed while `stop_hook_active` is true.
- Session state lives in `<data folder>/hooks/<session>.json` (written paths, warnings given, pairs reported) and is removed after 7 days. Writes go through a temp file and a rename under an advisory `<session>.lock` file (`create_new`, about 200 ms of retries, stale after 5 s).
- `formslot install <repo> [--mode warn|deny] [--enable] [--dry-run]` merges the six hooks and `enabledMcpjsonServers: ["formslot"]` into `<repo>/.claude/settings.json`, adds the `formslot` server to `<repo>/.mcp.json`, and writes `<repo>/slots.lock.json` through `slot apply` when the repository has none. It edits the files as text: existing keys keep their order, values and layout, a hook list that already holds a formslot command is left alone, a file keeps its line endings (all-CRLF stays CRLF), its mode and a leading UTF-8 byte order mark, a file that is not valid JSON, or a merge that would change an existing value, is refused with nothing written, and a file with comments is refused with a message saying that comments could be lost. When `disabledMcpjsonServers` lists `formslot` the report says so and the entry is left; `--enable` removes only that entry. It writes no other file.

## Later, fixed now so clients can plan

These land in later phases; names and shapes are fixed here and change only with a revision line.

- Write check over HTTP (reserved): `POST /api/check` with `{"tool": "Write" | "Edit", "path": "...", "content": "...", "session": "..."}` answering `{"action": "allow" | "warn" | "deny", "message": "...", "slots": [SlotRef]}`. The v0.9.0 check runs in the command line (section "Claude Code hooks"); the route is not served yet.
- Excerpt envelope: every piece of indexed text sent to a model is `{"kind": "formslot.excerpt", "source": {"repo", "path", "commit", "lines"}, "slot": "id or null", "licence": "SPDX or unknown", "copy": "allowed | notice | no_copy", "text": "..."}` and is data, never instructions.

## Revisions

- 1 · 2026-10-30 · first version, written in Phase S1
- 1 · 2026-10-30 · v0.2.1, compatible tightening after review: duplicate checked headers, targets without a leading slash and relative corpus paths are 400; `Corpus.remote` never carries credentials
- 1 · 2026-10-30 · v0.4.0, additive: the symbol, refs, dupes and drift commands, with their unit kinds, edge kinds, tiers, drift states and the stable id form
- 1 · 2026-10-30 · v0.4.1, conformance: Rust constants are reported as `constant`, as this file always said (v0.4.0 reported them as `const`); `--limit 0` means no limit for clone groups too
- 1 · 2026-10-30 · v0.5.0, additive: the related band (`related_pair`, reasons `similar` and `same_name`) and `dupes --related`; the slot manifest schema, the `slot` commands, `GET /api/slots` and `GET /api/slots/<id>`, the `slots.lock.json` format and the per-file drift states; no existing route, command or field changes
- 1 · 2026-10-30 · v0.7.0, additive: memory records (`memory_record`, `memory_event`) and the `memory`, `recall` and `remember` commands with their kinds, authorities and statuses; no existing route, command or field changes
- 1 · 2026-10-30 · v0.7.0, additive: search, retrieval and context packs (`search`, `map`, `pack`, `search rebuild`; tables `search_unit`, `unit_fts`, `unit_tri`, `memory_fts`; the pack JSON shape and P0/P1/P2 layout); no existing route, command or field changes
- 1 · 2026-10-30 · v0.7.1, conformance after review: memory text is redacted before it is stored and pack text before it is counted; `recall` leaves out proposals and caps `--limit` at 50; a pack whose P0 would take more than half the budget is sent as T1 so `sent` stays within `budget`; CC-BY-SA is `no_copy`; owner records are guarded by migration 010 (`memory_owner_no_edit_more`, `memory_owner_supersede_needs_successor`, `memory_owner_status_known`)
- 1 · 2026-10-30 · v0.9.0, additive: the MCP server (`POST /mcp`, `formslot mcp`, eight `slot_*` tools, `slot://` resources, three prompts, argument completion, cursor paging, the 25,000-token cap); `formslot check`, `formslot hook <event>` and `formslot install` with their hook JSON in and out, the session state folder `hooks/` and the Claude Code plugin under `plugin/formslot/` (the reserved `POST /api/check` is still not served); `formslot memory declines [--check <corpus>]` and the decline specs as data (`data/declines.json`, optional override `declines.json` in the data folder); `recall` matches whole words; `remember --propose` is one transaction and a NOOP adds an event to the existing record (`created` in the answer); `map --formmap --tokens N` shrinks its sections or exits 1 naming the minimum; a source-gone record stays stale when its anchor returns; the 26th first-load slot `ui.prefs-dialog`; no existing route, command or field changes
- 1 · 2026-10-30 · v0.9.1, conformance after review: `recall` compares light stems (plural forms match); a decline spec applies only to its `corpus`, and a trigger under 4 characters makes the decline data invalid; `check` treats text already in the file as old text for Edit and MultiEdit, and scans code files named like documents; `install` refuses symbolic links on `.claude` and its targets and plans `slots.lock.json` before it writes; `slot_get` detailed keeps its fields (the manifest leaves out `copies`, which are paged)

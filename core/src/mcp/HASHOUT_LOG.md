# HASHOUT_LOG · core/src/mcp · formslot

Living log for the MCP server (core/src/mcp/*, cli/src/mcp_cmd.rs, the `POST /mcp` route). Source files stay comment-free; what they would have said lives here. The core-wide log is core/HASHOUT_LOG.md.

Current build version: v0.10.1
Last updated: 2026-10-30 · Phase S8

### core/src/mcp/mod.rs · answer

- refers-to: core/src/mcp/mod.rs :: answer
- kind: rationale
- logged: v0.9.0 · 2026-10-30

Adapted from the owner's formMCP `core/src/mcp.rs` (GPL-3.0, NodeAgentc, Inc., the same owner; the protocol list, the `initialize` negotiation, `answer`, and the newline-delimited stdio loop come from there). Changes: no gateway state (formslot's tools are stateless over the store), batches handled by `answer_text` for both transports, requests with a null id and responses from the client are answered with nothing, tool failures become `isError` results while an unknown tool is a `-32602` protocol error (A07: execution errors reach the model, protocol errors do not). The negotiated version set is formMCP's: `2025-11-25`, `2025-06-18`, `2025-03-26`, `2024-11-05`, default `2025-06-18`. The 2026-07-28 revision in A07 drops `initialize` and `Mcp-Session-Id`; Claude Code 2.1.314 still opens with `initialize`, so this build serves the stateful form and the stateless form is a later revision line.

### core/src/mcp/mod.rs · initialize

- refers-to: core/src/mcp/mod.rs :: initialize
- kind: explainer
- logged: v0.9.0 · 2026-10-30

The `instructions` string is the one place an agent is told when to call `slot_find` (before writing a component) and that everything returned is data. Capabilities declare `completions` and no `listChanged` or `subscribe`: the tool set is fixed and resources change only on reindex, which a client sees by listing again.

### core/src/mcp/http.rs · post

- refers-to: core/src/mcp/http.rs :: post
- kind: rationale
- logged: v0.9.0 · 2026-10-30

JSON responses only, no SSE stream: every formslot call is request and response, so Streamable HTTP's JSON mode is complete. `Mcp-Session-Id` is issued at `initialize` because clients written to the 2025-03-26 and 2025-06-18 specs look for it, but it is not required back: `http::Request` carries no headers, a service restart would invalidate every session, and no tool keeps per-session state. The token, Host and Origin checks already ran in `http::handle` before `api::route` reaches this function, so `/mcp` gets exactly the checks of every other route. `GET` and `DELETE` answer 405 with `Allow: POST`, which the spec allows for a server with no stream and no session teardown.

### core/src/mcp/stdio.rs · serve

- refers-to: core/src/mcp/stdio.rs :: serve
- kind: rationale
- logged: v0.9.0 · 2026-10-30

In-process, not "start the daemon and forward". The plan (S8) modelled the launcher on formvis `setup/mcp_stdio.py`, which forwards. Here every tool is a read of the SQLite store (WAL, 5 s busy timeout) except `slot_reindex`, which takes the same in-process lock as the service; so a stdio server needs no token, no discovery file, no port and leaves no daemon behind when the agent exits (a daemon started by an MCP launch would outlive the session and keep a port open). Cost: two processes may write the store at once if a daemon is also running and both reindex; SQLite serialises them and the second waits up to 5 s. Stdout carries only JSON-RPC lines; the only diagnostic is one stderr line from the CLI at start. Nothing in `core` prints to stdout.

### core/src/mcp/page.rs · text_result

- refers-to: core/src/mcp/page.rs :: text_result
- kind: rationale
- logged: v0.9.0 · 2026-10-30

The single exit for every tool result. It redacts text blocks with `redact::text` and the structured value with `redact::value`, then enforces the 25,000-token cap with `ceil(bytes / 3.6)` over text plus structured JSON (the contract's code estimator, the conservative one). If the structured part alone is over half the cap it is replaced by `{"truncated": true}`; text is then cut at a line boundary and a note is appended. This is a backstop: lists stop at 20,000 tokens (`PAGE_TOKENS`) on their own, and `slot_context_pack` is bounded by its 16,000-token budget.

### core/src/mcp/page.rs · encode

- refers-to: core/src/mcp/page.rs :: encode
- kind: explainer
- logged: v0.9.0 · 2026-10-30

A cursor is the hex of `kind|print|offset`, where `print` is the first six bytes of a SHA-256 over the arguments that shape the list (query, corpus, format; never the page size, so a client may change `limit` between pages). Hex avoids a base64 crate. A cursor from another tool or other arguments is refused rather than silently restarting. The list is recomputed on each page (search is deterministic for an unchanged store), so a cursor holds an offset, not a snapshot; a reindex between pages can shift entries, which the steering text does not promise against.

### core/src/mcp/page.rs · listing

- refers-to: core/src/mcp/page.rs :: listing
- kind: explainer
- logged: v0.9.0 · 2026-10-30

Renders items one at a time and stops at the page size or the byte budget, whichever comes first, always keeping at least one item so a single huge item cannot make a page empty. The steering sentence names the remaining count and the cursor, as A07 asks, so a model knows to call again rather than assume the list ended.

### core/src/mcp/tools.rs · find

- refers-to: core/src/mcp/tools.rs :: find
- kind: rationale
- logged: v0.9.0 · 2026-10-30

`search::run` returns RRF-fused hits in which slots already carry weight 2.0; they are then stable-sorted slot-first so "slots first-class" holds even when a unit scores above a slot. Up to 200 hits are kept. Excerpts (detailed only, five per call, 40 lines each) are read from the git blob by the unit's byte range, redacted with `redact::text`, and wrapped in the `formslot.excerpt` envelope with the licence of the file and the copy verdict of `pack::copy_verdict`; `slot` is `null` there because slot membership of an excerpt is a pack concern (`pack` derives it from the file list).

### core/src/mcp/tools.rs · usages

- refers-to: core/src/mcp/tools.rs :: usages
- kind: explainer
- logged: v0.9.0 · 2026-10-30

A target that is a slot id (`manifest::is_id` and present in the store) is a slot; anything else is a symbol, so a dotted symbol that happens to look like a slot id still resolves as a symbol when no such slot exists. Symbol matching is the `refs` command's rule: exact `to_qualified`, or ending in `.name` or `::name`, over the `edge` table; `path#symbol` matches `to_path` and `to_qualified` exactly. For a slot, references are edges into its files from outside them, then the live copies from `copies::derive` (the same function `slot refresh` stores).

### core/src/mcp/lookup.rs · near_unit

- refers-to: core/src/mcp/lookup.rs :: near_unit
- kind: explainer
- logged: v0.9.0 · 2026-10-30

The unit-level counterpart of `slots::copies::derive`, which works on whole files: the same six sources (`clone_member` groups, `clone_pair` both sides, `related_pair` both sides) filtered to one `qualified` name, with the same relation names and ordering (T1, T2, T3, related-similar, related-same_name, then score). It is not stored; it is computed per call, so it is current after a reindex where a slot's stored `copies[]` is stale until `slot refresh`.

### core/src/mcp/tools.rs · parse_unit

- refers-to: core/src/mcp/tools.rs :: parse_unit
- kind: explainer
- logged: v0.9.0 · 2026-10-30

Accepts `path#qualified` and the stable unit id `formslot <repo> <path>#<qualified>~<sig8>` that `slot_find` and `show` print, so an agent can paste an id from a hit. The `~sig8` tail is dropped (eight hex characters only) and the repo word selects the corpus.

### core/src/mcp/tools.rs · reindex

- refers-to: core/src/mcp/tools.rs :: reindex
- kind: warning-context
- logged: v0.9.0 · 2026-10-30

The only write tool. `ref` is checked before it reaches git (no leading `-`, no whitespace or control characters, 200 characters at most) so a model-supplied value cannot become a git option. It takes `api::INDEXING`, the lock `POST /api/corpus/<id>/reindex` uses, so a reindex through MCP and one through HTTP in the same process never overlap. It does not run `slot refresh`; the result says so.

### core/src/mcp/tools.rs · owner_records

- refers-to: core/src/mcp/tools.rs :: owner_records
- kind: rationale
- logged: v0.9.0 · 2026-10-30

Owner rules and declines are read through `recall::recall` (limit 50, the recall maximum) and filtered to authority `owner` or kind `declined`, so a topic ranks them the way `formslot recall` does (a declined record whose trigger is in the topic first) and proposals and superseded records never appear. With no topic the 50 cap can hide some owner rules (formvis holds 52); the header counts what is shown and a topic narrows. Shared by the `conventions` tool and the `conventions` prompt.

### core/src/mcp/resources.rs · read

- refers-to: core/src/mcp/resources.rs :: read
- kind: explainer
- logged: v0.9.0 · 2026-10-30

`slot://slots/{id}` returns what `GET /api/slots/<id>` returns; `slot://memory/{id}` returns the stored record (already redacted at insert); `slot://map/{corpus}` is the `--formmap` text after `search::ensure`. Every body passes `redact::text` and is cut under the cap with a note. A missing resource is `-32002` with the URI, the code the MCP resources spec names.

### core/src/mcp/prompts.rs · get

- refers-to: core/src/mcp/prompts.rs :: get
- kind: explainer
- logged: v0.9.0 · 2026-10-30

Three prompts, all user-role text: `reuse-check` is the fixed find, follow, decide procedure (conventions first, so a declined task stops early); `pack-for-task` runs `pack::run` at 4000 tokens and sends the pack as a second message, labelled data; `conventions` lists the owner records of a corpus. Missing arguments are `-32602`.

### core/src/mcp/completion.rs · complete

- refers-to: core/src/mcp/completion.rs :: complete
- kind: explainer
- logged: v0.9.0 · 2026-10-30

Prefix completion for the three resource templates and the corpus argument of two prompts. `task` is free text and completes to nothing. At most 100 values, with the true `total` and `hasMore`, per the spec.

### core/src/mcp/schema.rs · tools

- refers-to: core/src/mcp/schema.rs :: tools
- kind: rationale
- logged: v0.9.0 · 2026-10-30

Descriptions say when to call a tool and what to call next (A07: tool descriptions are the steering surface; "call this BEFORE writing a new component"). No `outputSchema` is declared: the spec then obliges the server to conform and the structured shapes vary by `response_format`; the contract documents them instead. `additionalProperties: false` on every input so a typo is visible to schema-validating clients. `slot_reindex` is the only tool without `readOnlyHint`.

### core/src/http.rs · Reply

- refers-to: core/src/http.rs :: Reply
- kind: rationale
- logged: v0.9.0 · 2026-10-30

`headers` added so `/mcp` can send `Mcp-Session-Id` and `Allow`. Only `Reply::json` builds a `Reply`, so the field is set in one place; `with_header` is the one way to add to it. Header values are program-made (hex ids, fixed words), never client text, so there is no header injection path. `202 Accepted` joins the reason table for notification-only bodies.

### cli/src/mcp_cmd.rs · run

- refers-to: cli/src/mcp_cmd.rs :: run
- kind: explainer
- logged: v0.9.0 · 2026-10-30

Thin: refuses extra arguments (exit 2), prints one line to stderr, and hands stdin and stdout to `mcp::stdio::serve`. Listed in `formslot --help` with two leading spaces and the word `mcp` first, which subform's `^\s+mcp\b` check needs to put formslot in a session's MCP config.

### core/src/mcp/tools.rs · context_pack, owner_records

- refers-to: core/src/mcp/tools.rs :: context_pack, owner_records; core/src/mcp/page.rs :: listing
- kind: rationale
- logged: v0.16.0 · 2026-11-01

Found by running all nine tools in headless `claude -p` against a copy of the v0.15.0 store. (1) `slot_context_pack` returned the pack text as a content block and metadata as `structuredContent`; Claude Code hands the model only the structured part, so the model saw hits and hashes and no pack ("there is no prompt body field"). It now returns no `structuredContent`: the pack block and the summary block are the whole answer (13,289 characters, intact in the re-run). Same lesson as `slot_source`. (2) `owner_records` asked recall for the top 50 records of any kind and kept the owner ones afterwards, so a topic that hashout records match strongly (z-index, index) lost every owner rule and answered "0 records"; it now recalls kind `rule` and kind `declined` separately and merges by score (declined first on ties), so `z-index` returns 7 rules. (3) Structured results gained what only the text carried: `more` beside `next_cursor` on paged lists, `dependsOn` on `slot_get`, and the refresh note on `slot_reindex`. All nine tools were found through Claude Code's tool search and called with the right arguments on the first try; the other seven results reached the model unchanged (structured JSON, 99 to 5,000 characters; `slot_source` 15,281 characters of text).

### core/src/mcp/mod.rs · both eras: 2026-07-28 and the handshake revisions

- refers-to: core/src/mcp/mod.rs :: answer, modern, stamp, discover, initialize, unsupported; core/src/mcp/http.rs :: post, modern_headers, status_of; core/src/http.rs :: Request, handle
- kind: rationale
- logged: v0.17.0 · 2026-11-22

The stance, on the owner's word: dual-era. The server keeps every handshake revision it had and adds 2026-07-28, in one process and on both transports. The specification allows exactly this ("Versioning and Compatibility", 2026-07-28: a dual-era server selects its behaviour from how the client opens, and may serve both eras on one endpoint). Claude Code's own MCP page says its newer runtime asks a server whether it supports 2026-07-28 and otherwise connects as before, so a legacy-only server would keep working; dual-era is chosen so the server may be called current and so a client that speaks only the new revision is not turned away. All pages were read on 22 November 2026.

How the era is chosen: `initialize` always selects the handshake path. Any other request whose `params._meta` holds `io.modelcontextprotocol/protocolVersion`, and every `server/discover` (the probe a dual-era client sends first on stdio, which must answer even with no `_meta`), goes through `modern`. Everything else is served exactly as before. `modern` reuses `dispatch`, so a tool, a resource or a prompt has one implementation; the differences are applied around it: `stamp` adds `resultType: "complete"` and the server identity to every result and the `ttlMs` and `cacheScope` pair to the six cacheable results; `-32002` becomes `-32602` for a missing resource; the four methods the revision removed answer `-32601`; another version answers `-32022` with the supported list.

What was checked against the 2026-07-28 pages (changelog, versioning, discovery, Streamable HTTP): the `_meta` key names, the `server/discover` result shape, the three error codes (`-32020`, `-32022`, `-32601`) and their HTTP statuses (400, 400, 404), the three request headers and the Base64 form of `Mcp-Name`, no session id for stateless requests, and 405 for GET and DELETE (already so). Not checked against its own page: the caching page. `cacheScope` is "private" everywhere because the service sits behind a token on one computer, and `ttlMs` is 0 for resource lists and reads because an index run changes them; whether 0 is the right way to say "do not reuse" should be read on that page before anyone relies on it. Not implemented, on purpose: `subscriptions/listen` (the capability says `listChanged: false`), the tasks extension, input requests, and `x-mcp-header` on tool parameters (no tool needs a header).

Item 4.1, the handshake path: an unknown version was answered with 2025-06-18 although 2025-11-25 was in the server's own list. The lifecycle page says to answer with another supported version, which should be the latest. It now answers with 2025-11-25. That revision's changes for a server of this kind were read from its changelog and hold here: an invalid Origin gets 403 (`core/src/http.rs`), an input error in a tool call is a tool result with `isError`, not a protocol error (`call_tool`), logs go to stderr on stdio, and the schemas use no keyword outside JSON Schema 2020-12; icons, the `description` of `Implementation`, tasks and the elicitation and sampling changes are optional or belong to features this server does not offer. A request with no version keeps getting 2025-06-18.

Item 4.2: `core/src/http.rs` dropped the request headers before the route ran, so nothing could read `MCP-Protocol-Version`. `Request` now carries the headers (without `Authorization`, which is checked and then removed, so no route can log it). A handshake-era request with a version header outside the list is 400; with no header it is served as before, which is what a client older than 2025-06-18 sends.

Run on Linux: the stdio server with an unknown, a known and a missing version in `initialize`, `server/discover` with and without `_meta`, a wrong version, `ping` in both eras, a missing resource in both eras, `tools/list` and a tool call under 2026-07-28; and `POST /mcp` on a running service with fifteen requests covering every status above, the session header in the handshake era and its absence in the new one, 401 with no token and 403 for a foreign Origin. Not run: any real client. Claude Code, Codex and Cursor connecting under either era are steps in TEST_PLAN.md.

### core/src/mcp/curate.rs · slot_recall, slot_remember, slot_suggest, slot_drift

- refers-to: core/src/mcp/curate.rs :: recall, remember, suggest, drift; core/src/mcp/schema.rs :: tools, names, read_only_names; core/src/slots/suggest.rs :: report_json; cli/src/agents_cmd.rs :: run
- kind: rationale
- logged: v0.17.0 · 2026-11-22

Four things the command line did and an agent over MCP could not. Each tool calls the same core function as its command and returns, as `structuredContent`, the same JSON the command prints with `--json`, so the two cannot drift: `slot_recall` is `recall::recall`, `slot_remember` is `propose::propose`, `slot_suggest` is `suggest::suggest` with the JSON builder moved from the command into `suggest::report_json` and called by both, and `slot_drift` is `consumer::drift_lock` or `drift_infer`. Drift's command prints an array and a structured result must be an object, so the tool wraps it as `{repo, mode, result}`.

`slot_remember` is the one new tool that writes. It stores a proposal and nothing else, refuses `rule` and `declined` before it reaches the store, and the owner triggers of migration 010 still stand behind it. It is annotated like `slot_reindex`: not read-only, not destructive, idempotent (a repeat is a NOOP that adds an event). `slot_suggest` never writes: the command's `--write` has no tool argument. `slot_drift` reads a folder the caller names, so it takes only a git checkout or a folder that holds `slots.lock.json`, and it returns states and hashes, never file text.

Item 4.3: Claude Code writes a text result over 50,000 characters to a file unless the tool's `tools/list` entry sets `_meta["anthropic/maxResultSizeChars"]`, up to 500,000 (its MCP page, "Raise the limit for a specific tool"). `slot_source` can reach about 80,000 encoded characters and a T16 pack about 64,000, so both entries declare 200,000. The same page says a tool that declares the key is no longer held to `MAX_MCP_OUTPUT_TOKENS`; formslot's own 25,000-token cap in `page::text_result` and the source tool's byte caps still apply. Whether a 72 KiB source then reaches the model as text needs Claude Code and is a step in TEST_PLAN.md.

Item 4.4: `formslot agents sync` writes `agents/manifest.json` in the family's manifest shape (`format: form-agent-manifest`, as FormVis and FormPortal write theirs): the tool names, `read_only`, an empty `approval_gated`, and two additions, `titles` and `own_store` (the tools that write formslot's own store and nothing else, the family's third class). It reads the definitions in `schema.rs`, so a count is never typed: 13 tools, 11 read-only, 2 own-store, counted by the command. `--check` exits 1 while the file differs and runs in the Linux job of `ci.yml`.

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

# HASHOUT_LOG · core/src/memory · formslot

Living log for the memory records (core/src/memory/*, migration 008, cli/src/memory_cmd.rs). Source files stay comment-free; what they would have said lives here. The core-wide log is core/HASHOUT_LOG.md.

Current build version: v0.7.0
Last updated: 2026-10-30 · Phase S6

### core/src/corpus.rs · memory after an index run

- refers-to: core/src/corpus.rs :: index
- kind: rationale
- logged: v0.7.0 · 2026-10-30

`memory::after_index` (ingest plus the anchor check) runs when the index run changed something or the corpus has no memory records yet. Running it on every reindex took an unchanged formvis reindex from 46 ms to 5 s; a run that read no blob and removed no path cannot change a memory source or an anchor.

### core/src/memory/hashout.rs · tombstones from formvis's older log format

- refers-to: core/src/memory/hashout.rs :: entries
- kind: warning-context
- logged: v0.7.0 · 2026-10-30

formvis's logs use `## title · _vX (files)` entries rather than `### path · symbol` with refers-to lines, and most of them say "Removed" somewhere, which this ingest reads as removed code: formvis gives 3,484 tombstones of 6,300 records. Recall still ranks by text, so they cost tokens only when they match; tighten the tombstone test (the `kind: removed-code` line, or "Removed:" at the start of a sentence) when the S7 packs show them crowding out live entries.

### review fixes (v0.7.1)

- refers-to: core/src/memory/mod.rs :: insert; core/src/memory/recall.rs :: recall, RECALL_LIMIT_MAX; core/src/migrations/010_memory_owner_guard.sql
- kind: bug-context
- logged: v0.7.1 · 2026-10-30

From the fresh-context review of 1a343f5..v0.7.0. `insert` redacts title and text before storing (a `ghp_` token in a CLAUDE.md rule reached P0, `recall` and `memory list` verbatim) while `content_hash` stays over the source text, so reconcile still pairs an unchanged line with its record. `recall` skips `proposed` records: an agent proposal saying the opposite of an owner decline ranked second for "seam carving". The anchored budget loop kept recounting every record on every pass and stepped one character at a time between equal lengths (9.7 s for `--limit 50`); it now caches each record's cost, steps by at least 16 characters or the excess spread over the records, and `--limit` is held to 50 (5 ms for the same call). Migration 010 widens the owner guard: anchor, triggers, hash, corpus and dates are fixed, status moves only among active, stale and superseded, and superseded needs a successor; ingest's own updates (source, status, the supersede with `superseded_by`) still pass, checked by superseding a changed CLAUDE.md line and by direct UPDATEs that the triggers refuse.

Known and left for S8: the decline list in declines.rs is hand-kept data (CLAUDE.md wants family lists generated; move it to a data file with a drift check or derive it from a "Declined:" convention); a record whose source line vanished while it was anchor-stale can come back to active when the symbol returns; recall scores by substring, so one-letter terms match broadly; `remember --propose` writes without a transaction and a NOOP still adds a row; owner authority is any CLAUDE.md or AGENTS.md at any depth (an agent that edits one mints owner rules); an unchanged re-ingest of formvis takes about 4 s (check_anchors runs twice).

## memory

### memory/mod.rs · record shape and event log
- refers-to: core/src/memory/mod.rs :: Record, NewRecord, insert, event, list, counts, content_hash
- kind: rationale
- logged: v0.7.0 · 2026-10-30

The record follows A06's bi-temporal model reduced to what the table holds: `valid_from`/`valid_to` and `superseded_by` for validity, `created_at`/`updated_at` for system time. `content_hash` covers kind, authority, anchor, title and text and not the source line, so a bullet that only moves down a file is a change of `source`, not a new record. Every status change writes a `memory_event` row (actor `ingest`, `anchors` or `agent`). `list` hides superseded records unless asked, because recall should return the current truth; `memory list --status superseded` shows history.

### migrations/008_memory.sql · triggers
- refers-to: core/src/migrations/008_memory.sql :: memory_owner_no_edit, memory_owner_no_promote, memory_record_no_delete
- kind: rationale
- logged: v0.7.0 · 2026-10-30

The owner guard is a trigger on `UPDATE OF text, title, kind, authority` when the old row is owner, so status, valid_to, superseded_by, source and updated_at can still move (supersession and staleness need them). A second trigger refuses turning any record into an owner one; an owner record exists only by being inserted. The trigger fires on any statement that names those columns, so the code never names them in an UPDATE of a record it did not create.

### memory/markdown.rs · blocks
- refers-to: core/src/memory/markdown.rs :: blocks, sections
- kind: explainer
- logged: v0.7.0 · 2026-10-30

A small line parser, not a Markdown engine: fenced code is skipped, table rows are skipped, a list item swallows its lazy continuation lines, and a block carries the trail of headings above it. Rules come from items and paragraphs under a level-2 or deeper heading, so a file's introduction (before the first `##`) is not a rule. Line numbers are 1-based and point at the first line of the bullet, which is what `source` records.

### memory/ingest.rs · sources and classes
- refers-to: core/src/memory/ingest.rs :: classify, gather, rule_items, build_items, dev_wise_items
- kind: rationale
- logged: v0.7.0 · 2026-10-30

Files are found in the `file` table and their blobs read through `BlobReader` at the indexed oid, so memory always matches the indexed commit, never the working tree. Classes: CLAUDE.md and AGENTS.md rules (owner); `.claude/skills/*/SKILL.md` numbered items under a heading with "law" (owner; the harness laws); NOTICE.md items and paragraphs under headings naming keeping, convention, current or trademark (owner); `HASHOUT_LOG*.md` (derived); BUILD_NOTES.md items under a `Decisions` heading as `decision` and dev-wise.md sections as `how-to` (both derived, because they are written by the build pipeline and not the owner's own rules file). Mirrors (`.agents/`), vendored and generated files never count.

### memory/ingest.rs · reconcile
- refers-to: core/src/memory/ingest.rs :: reconcile, stale_by_source
- kind: rationale
- logged: v0.7.0 · 2026-10-30

Records are grouped by source file and kind. Stage 1 pairs by content hash (unchanged; a moved line only updates `source`). Stage 2 pairs a leftover new item with a leftover old one by equal title and anchor, else by the same anchor and at least half the same words; the pair is a change, so the new item is inserted and the old record becomes `superseded` with `valid_to` and `superseded_by`. Whatever is left over: new items are added; old active records become `stale`, never deleted. A stale record comes back only when the same content returns and its last event was a source-level stale, so an anchor-stale record is never revived by ingest. `check_anchors` runs before reconcile (so a deleted symbol is reported as an anchor loss and not as a vanished source line) and again after it (so new records get their `anchored` event).

### memory/hashout.rs · two log formats
- refers-to: core/src/memory/hashout.rs :: parse, targets, legacy_anchors, candidate_names, resolve
- kind: rationale
- logged: v0.7.0 · 2026-10-30

The new format (`### path · symbol` with `- refers-to:` lines, as in formslot's and formMCP's logs) is read as written: groups split on ` · `, `path :: a, b` gives one anchor per symbol, a path with no `::` or a symbol with spaces gives a path anchor. formvis's logs predate that format: `## title · _vX (files)` and `### path · symbol · _vX`. For those the files come from the heading, are resolved against the log's folder (frontend/src/...) or by a unique basename, and the symbols are the code-shaped words (camelCase or snake_case) and backticked names in the heading and body that are units of those files. That is how `revealApp` in an entry that never writes `ui/dom.ts#revealApp` still anchors to it. At most 16 anchors per entry. An entry with none becomes one record with no anchor, still found by text. A legacy entry is a `tombstone` when its body has a sentence or bullet starting with a capital `Removed`: the old logs quote removed code that way (716 of 1,344 entries in the two formvis logs), and "No removed code" in lower case does not match.

### memory/declines.rs · SPECS
- refers-to: core/src/memory/declines.rs :: SPECS, scan
- kind: rationale
- logged: v0.7.0 · 2026-10-30

The decline list is data: a title, the substrings that find the owner's own line, the substring that picks the sentence(s) to store, and the trigger phrases. The stored text is the owner's wording from the first match in CLAUDE.md, then AGENTS.md, then skill files, then HASHOUT logs, then CHANGELOG, and `source` is that `path:line`; nothing is paraphrased. A spec with no match in a corpus makes no record there, and a record whose line later disappears goes stale like any other. Found in formvis: Content-Aware Scale by seam carving (CLAUDE.md:15), Poisson blending and live fill (same line, a separate sentence), one cloud provider (CLAUDE.md:17, with the provider names as triggers from harness law 11), the sparkle icon (CLAUDE.md:18), Adobe feature names (CLAUDE.md:16), the painted sampling area (CLAUDE.md:23), the old product name (CLAUDE.md:25), the Liquify face mesh (AGENTS.md:13). formMCP adds its two owner declines (no third-party MCP servers, no install tool) when its corpus is ingested. The rules keep their full bullets too, so a decline is a focused view of a rule and not the only copy.

### memory/anchors.rs · check_anchors
- refers-to: core/src/memory/anchors.rs :: check_anchors, symbol_matches
- kind: rationale
- logged: v0.7.0 · 2026-10-30

An anchor resolves when the file exists at the indexed commit and a unit has the same qualified name (`::` and `.` are the same separator, a trailing parenthetical is ignored) or, for a bare symbol, the same name. A record goes stale only if its anchor resolved before (an `anchored` event exists): refers-to targets such as `Settings.secrets` (a field, not a unit) or prose like "token remembered" never resolve and must not be stale from birth. A stale record whose last event is an anchor loss becomes active again when the symbol returns (a rename and back, a revert).

### memory/propose.rs · propose
- refers-to: core/src/memory/propose.rs :: propose
- kind: rationale
- logged: v0.7.0 · 2026-10-30

Cheap rules only, as A06 suggests before an LLM judge: NOOP when the normalised words equal any live record's; MERGE when the anchor (or both absent) and kind match and the word overlap (Jaccard) is at least 0.8; SUPERSEDE when the anchor is set and a live record of the same kind sits there with different text; otherwise ADD. The proposal is always stored (status proposed, authority agent, source `proposal`) with an event naming the action and the target, so the review queue is complete. The action is advice: nothing is merged, superseded or edited, and when the target is an owner record the note says only the owner supersedes it. Text passes `redact::text` first. Only decision and how-to may be proposed.

### memory/recall.rs · recall
- refers-to: core/src/memory/recall.rs :: recall, score_of, ANCHOR_BUDGET
- kind: rationale
- logged: v0.7.0 · 2026-10-30

Stand-in for the search agent's FTS: a term score (title 3, triggers 3, anchor 2, text count up to 5 per term, phrase bonuses), owner +1, stale halved. A declined record with a trigger phrase found in the query on word boundaries gets +1000, so it is first. `--anchor` filters exactly on the anchor and then fits 2,000 tokens, reserving 220 characters for the footer: the longest text is cut toward the next longest (never under 200 characters), the cut is marked with its own shown/total count, and only if every text is at the floor are the lowest-ranked records left out, counted in the footer note. The total is the estimate of the records block (characters / 4, rounded up).

### cli/src/memory_cmd.rs · commands
- refers-to: cli/src/memory_cmd.rs :: run, ingest, list, recall, remember
- kind: explainer
- logged: v0.7.0 · 2026-10-30

Thin: argument parsing and printing. `memory ingest` prints the report and a count table per kind and status; `memory list --count` prints the same table for the whole store. `recall` takes the words as the query and defaults to 5 records.

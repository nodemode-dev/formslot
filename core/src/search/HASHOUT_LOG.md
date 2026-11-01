# HASHOUT_LOG · core/src/search · formslot

Living log for search, the repo map and context packs (core/src/search/*, migration 009, cli/src/search_cmd.rs). Source files stay comment-free; what they would have said lives here. The core-wide log is core/HASHOUT_LOG.md.

Current build version: v0.10.1
Last updated: 2026-10-30 · Phase S7 review

### review fixes (v0.7.1)

- refers-to: core/src/search/pack.rs :: clean, title_and_text, build, copy_verdict, slot_card, memory_line, build_p0; core/src/search/map.rs :: formmap; core/src/search/index.rs :: ensure; cli/src/search_cmd.rs :: run
- kind: bug-context
- logged: v0.7.1 · 2026-10-30

From the fresh-context review of 1a343f5..v0.7.0. Nothing in a pack passed the second secret scan: excerpt text, rule and memory lines, slot cards, the task line and FORMMAP rule titles now go through `redact::text` before they are counted (checked with an `sk-ant-` and an `AKIA` key in a function body and a `ghp_` token in CLAUDE.md: all three print as `[redacted by formslot]`). Rule titles are "heading: first words", so `text.starts_with(title)` never held and every rule printed its opening twice; `title_and_text` finds the ": " split whose tail starts the text and prints heading plus text once (formvis P0 now carries 7 owner rules in the same 350 tokens, up from 5; P0 is still byte-identical across runs and tiers, new hash 7e2e8ed4...). A pack whose P0 would take more than half the budget falls back to T1, and the task line is clipped to what is left, so `--tier T16 --tokens 100` sends 5 tokens instead of 1,051. CC-BY-SA is `no_copy`. `index::ensure` skips a corpus with no extracted units and one whose repository can no longer be opened, so one moved docs-only corpus no longer breaks every search. `search rebuild` is the rebuild only when it is the whole query.

Closed in v0.9.0: `--formmap --tokens N` shrinks its sections or errors with the minimum, and `params_of` keeps arrow types in parameter lists.

### core/src/migrations/009_search.sql · search_unit, unit_fts, unit_tri

- refers-to: core/src/migrations/009_search.sql :: search_unit
- kind: rationale
- logged: v0.7.0 · 2026-10-30

`unit` rows belong to a blob, not to a corpus, and carry no text; the same blob can sit at several paths and in several corpora. `search_unit` is the per-corpus, per-path copy of the fields a search needs (kind, name, qualified name, signature, byte and line range) and gives each row an integer id that is also the FTS rowid. `unit_fts` is contentless with `contentless_delete = 1` (SQLite 3.43 and later; the bundled SQLite is 3.53) so the body text, which would double the store, is never kept: it is read from git at index time and only its tokens are stored. The `search_unit_gone` trigger deletes the FTS rows when a `search_unit` row goes, so code that removes rows (a changed blob, a removed file, `search rebuild`) cannot leave stale index entries. `unit_fts` uses the porter stemmer over unicode61 so "dialogs" finds "Dialog"; its `words` column holds the qualified name split at camelCase and snake_case boundaries, because unicode61 keeps `SettingsDialog` as one token. `unit_tri` uses the trigram tokenizer over name, qualified name and path for substring lookups of three characters or more (A04). Bundled SQLite FTS5 was confirmed by building and running the migration.

### core/src/migrations/009_search.sql · memory_fts

- refers-to: core/src/migrations/009_search.sql :: memory_fts
- kind: rationale
- logged: v0.7.0 · 2026-10-30

External-content FTS5 over `memory_record(title, text, triggers)` with the three standard triggers, so every code path that writes a record (ingest, propose, the owner's changes, now or later) keeps the index in sync without being edited. The delete trigger is there for completeness: the memory migration refuses `DELETE` on records. The final `rebuild` statement indexes any records that existed before this migration ran. This migration must stay after 008, which creates `memory_record`.

### core/src/search/index.rs · sync

- refers-to: core/src/search/index.rs :: sync
- kind: rationale
- logged: v0.7.0 · 2026-10-30

Called from `corpus::index` inside the index transaction, after the file table and licences are settled. It deletes `search_unit` rows whose (path, oid) is no longer in `file`, then adds rows for every file that has units and no rows yet, reading each new blob once through `git cat-file --batch` and slicing it by the unit byte range. A reindex that changes nothing opens no blob reader. Bodies are capped at 8,192 bytes, and at 1,536 bytes for containers (class, namespace, mod, struct, trait, interface), whose members are separate units: without the cap a class would outrank its own methods for every term. A blob that cannot be read is indexed with an empty body (name, signature and path still match). `ensure` runs the same sync lazily for a corpus indexed before v0.7.0, so an existing store needs no manual step; `rebuild` is `search rebuild`.

### core/src/search/text.rs · query_terms

- refers-to: core/src/search/text.rs :: query_terms
- kind: rationale
- logged: v0.7.0 · 2026-10-30

Tasks are written as sentences ("add a section to the Preferences dialog"). The query is split at word and camelCase boundaries, lower-cased, stripped of a short stop list that includes the task verbs (add, make, create, implement, write, use), de-duplicated and cut at 12 terms, then sent to FTS5 as quoted terms joined by OR so no user text can reach the FTS query syntax. Trigram terms keep path-like tokens whole (`app-dialogs.ts`) and add words of four letters or more. `stem` is a small plural stripper used only for the slot text ranking, which is done in Rust, not FTS.

### core/src/search/lexical.rs · lexical

- refers-to: core/src/search/lexical.rs :: lexical
- kind: rationale
- logged: v0.7.0 · 2026-10-30

bm25 column weights: name 8, split name 6, signature 2, body 1 for `unit_fts`; name 5, qualified 3, path 2 for `unit_tri`; title 6, text 2, triggers 4 for `memory_fts`. They are proposals to be tuned against the gold set of Phase S10. Ties are broken by path and qualified name so the order is the same on every run. A unit with overloads in one file appears once.

### core/src/search/lexical.rs · neighbours

- refers-to: core/src/search/lexical.rs :: neighbours
- kind: rationale
- logged: v0.7.0 · 2026-10-30

The graph list takes the eight best fused unit hits as seeds and follows `edge` both ways (what the seed uses and who uses the seed), counts links per neighbour, and ranks by link count, then path and name. Edges with an empty target name (file-level imports) are skipped because they do not name a unit.

### core/src/search/mod.rs · fuse_weighted

- refers-to: core/src/search/mod.rs :: fuse_weighted
- kind: rationale
- logged: v0.7.0 · 2026-10-30

RRF with k = 60 (A04). Plain RRF buries slots: there are few of them, so even the best slot has one list and a score of 1/61 against 2/61 for a unit found by two lists. The lists carry weights: lexical, trigram and graph 1.0, owner-class memory (rule, decision, how-to, declined) 2.0, derived memory (hashout, tombstone, thousands of rows in formvis) 0.7, slots 2.0 and only the best five. Reuse of a canonical component and the owner's declines are the point of formslot, so they surface above loose code matches. Units in test paths (`e2e/`, `tests/`, `test_*`, `*.test.*`, `*.spec.*`) are multiplied by 0.5, because formvis has more e2e test units that mention a feature than code units. All of these are tuning constants to revisit with the S10 evaluation.

### core/src/search/slots.rs · rank

- refers-to: core/src/search/slots.rs :: rank
- kind: rationale
- logged: v0.7.0 · 2026-10-30

Slots are read from the `slot` table at query time (tens of rows) instead of being indexed, so no write path has to keep a slot index in sync; only canonical and proposed slots take part. Two lists are fused: a BM25 over a weighted bag of words (id, title and tags weigh 3, file paths and exported symbol names 2, summary and description 1) and a file list that scores a slot by the rank of the top ten fused unit hits that sit in its files (CSS selector, id and custom-property units are ignored, they only echo class names). A slot whose files hold the best code leads; with no such evidence the text order decides, and on equal fused scores the higher text score wins, then canonical over proposed, then id. For "add a section to the Preferences dialog" the lead is `ui.prefs-store`: the Preferences dialog class itself (`SettingsDialog` in `app-dialogs.ts`) is in no slot, so its methods lead the unit list instead.

### core/src/search/rank.rs · pagerank

- refers-to: core/src/search/rank.rs :: pagerank
- kind: rationale
- logged: v0.7.0 · 2026-10-30

Plain power iteration, damping 0.85, a fixed 40 iterations, dangling mass spread evenly, nodes ordered by (path, qualified name) and edges by a sorted merge, so the floating point sums happen in the same order on every run and the ranking is reproducible. Nodes are the corpus's units plus one file node per path; an edge to a name that is not a unit falls back to its file node, an edge to a path outside the corpus is dropped. Edge weight is the kind weight times the square root of the repeat count (Aider's damping of repeated references).

### core/src/search/map.rs · rank_entries

- refers-to: core/src/search/map.rs :: rank_entries
- kind: rationale
- logged: v0.7.0 · 2026-10-30

Entries are exported units except methods, CSS units, names starting with an underscore and anything in a test path (A02: no private names, no bodies). A unit's score is its own rank, plus the rank of its methods (so a class collects what its methods are used for), plus an even share of its file's rank. `fit` is the Aider loop: binary search for the largest prefix of the ranking whose rendering fits the token budget (and, for FORMMAP, the line limit). Rendering is grouped per file in order of first appearance, members in line order, one line per file cut at 160 characters at a member boundary.

### core/src/search/map.rs · formmap

- refers-to: core/src/search/map.rs :: formmap
- kind: rationale
- logged: v0.7.0 · 2026-10-30

FORMMAP.md is imported whole by CLAUDE.md, so the generator holds the size (A02): at most 148 lines, 4,096 tokens by default. It lists up to 12 owner rules and 30 slots before the map, and gives the map whatever lines remain. There is no timestamp, so regenerating on an unchanged index gives the same bytes and git shows no change.

### core/src/search/pack.rs · build_p0

- refers-to: core/src/search/pack.rs :: build_p0
- kind: rationale
- logged: v0.7.0 · 2026-10-30

P0 must be byte-identical across calls for one corpus head and one memory version (A10), and across tiers and tasks. It therefore holds only: a fixed notice, the corpus name and head, the owner rules and a short hash of their ids and content hashes (the memory version), and the map head fitted to a fixed 650 tokens; rules stop at 350 tokens with a "N more" line. Nothing in it depends on the task, the time, a path outside the corpus or the formslot version. A10 wants P0 of 4,096 tokens or more to be cacheable on every model; this P0 is about 1,000, which caches on models with a 1,024 minimum and is a known gap (raise `P0_MAP_TOKENS` when the pack goes to a model that needs more).

### core/src/search/pack.rs · build

- refers-to: core/src/search/pack.rs :: build
- kind: rationale
- logged: v0.7.0 · 2026-10-30

Budget is for what is sent: P0 plus P1 plus P2. T1 is "P2 only" in A10, so for T1 P0 and P1 are not sent (`included: false`) while the P0 hash is still reported, and P2 names the best slot ids in one line. P1 may use 35 percent of what P0 leaves; P1 slots are listed best match first and are never sorted by status, so the slot whose files match best leads. P2 adds memory records first (owner declines must not be cut), then excerpts in fused order, skipping any unit whose line range overlaps one already taken in the same file; an excerpt that does not fit is halved down to six lines before it is skipped, and each cut states the lines actually kept in the envelope. The repo map lines and the P1 unit locations are index lines, not excerpts: they hold names and parameter names, never a body. Every body goes through the excerpt envelope, one JSON object per line, with `copy` from `copy_verdict` (B08's policy mapped to the contract's three words: copy_ok is `allowed`, credit and unmodified-only obligations are `notice`, link-only, no-copy and unknown are `no_copy`). `slot` is the first slot (in id order) whose origin repo is the corpus and whose files include the path. When the repository is not on disk the excerpts are skipped, not failed.

### core/src/search/tokens.rs · code

- refers-to: core/src/search/tokens.rs :: code
- kind: explainer
- logged: v0.7.0 · 2026-10-30

The estimate is `ceil(bytes / 3.6)` for code and envelopes (done as `ceil(bytes * 10 / 36)` in integers) and `ceil(bytes / 4)` for prose, per part, so a pack's number is the sum of its parts. It is an estimate and runs low against the Anthropic tokenizer for the 4.7 and later models by up to about 30 percent (A10); the pack budget leaves the 4,000-token pack at about 3,900 estimated tokens, and the exact count is the job of `--count remote`.

### cli/src/search_cmd.rs · pack

- refers-to: cli/src/search_cmd.rs :: pack
- kind: todo
- logged: v0.7.0 · 2026-10-30

`--count remote` would call the Messages `count_tokens` endpoint with the key from ANTHROPIC_API_KEY. The engine has no HTTP client crate (its server is std-only) and adding one (ureq or reqwest with TLS) would pull in rustls and ring, which is a decision for the owner and a DEPS.md row, so in v0.7.0 the flag is parsed, refuses to run without the key, prints a note and uses the local estimate. Nothing in this phase makes a network call. Plan: the key lives in the system password store through the `keyring` crate as formvis does (service `formslot`, account `anthropic`), ANTHROPIC_API_KEY is only a fallback for CI, the key is never written to a file or the log, and the per-model counts are stored in the pack keyed by model family (A10). The pack prompt goes to stdout and the summary (tier, P0 hash, token counts, timing) to stderr so a hook can pipe the prompt untouched.

### core/src/corpus.rs · index

- refers-to: core/src/corpus.rs :: index
- kind: warning-context
- logged: v0.7.0 · 2026-10-30

`crate::search::index::sync` runs in the index transaction after `refresh_licences`, so a failed sync rolls the whole index run back. Its cost on an unchanged reindex is one DELETE and one SELECT (formvis: 58 ms for a full reindex with nothing to read); a first index adds one blob read per file that has units. It must stay before `memory::after_index`, which does not depend on it, and after the units are written, which happens while entries are processed.

### core/src/search/mod.rs · retrieve_with

- refers-to: core/src/search/mod.rs :: retrieve_with
- kind: rationale
- logged: v0.10.2 · 2026-10-30

Owner rules and declines were never reaching the ranking for most rule questions: one memory query with a pool of 20 was shared with 6,000 HASHOUT and tombstone records, which are long and crowd out the short rules before the partition. The two kinds are now queried separately (`lexical::memory_by`). Putting rules straight into the fusion at weight 2.0 then flooded the top ranks of ordinary code tasks (R@1 fell from 0.340 to 0.245), so a rule enters the fusion only when it holds at least 30 percent of the task's stemmed terms (`lexical::covered`) and at most four rules enter. Tried and dropped on the tuning half: coverage 0.5 (rules missed, R@10 0.796) and 0.2 (R@1 0.279). Weight stays 2.0. Held-out effect: R@10 0.824 to 0.838, R@1 0.392 to 0.378 (rules sometimes outrank the right file on tasks that only mention a rule word).

### core/src/search/pack.rs · build_with

- refers-to: core/src/search/pack.rs :: build_with
- kind: rationale
- logged: v0.10.2 · 2026-10-30

`build` is split so a caller can hand in a `Retrieval` (the evaluation's dense arm passes one fused with the dense list). Two pack changes. First, P1 gets a `files:` line after the unit lines: the next files by summed fused score of their units (top 60 unit hits, tests excluded, files already shown skipped), up to 14 at T4 and 30 at T16, inside the same P1 room. A file path costs about 10 tokens, so it gives an agent more places to open for far less than an excerpt; the cost is precision, because file 15 is a weak candidate, and the excerpts get slightly less room (mean pack 3797 to 3833 tokens, all still under 4,000). This is the main source of the R@4k rise and should not be read as better ranking. Second, an owner rule that is already in the stable prefix P0 is restated in P2 when it is among the first five fused hits (at most two): P0 is a cache prefix far from the task, and a rule question is answered best by the rule next to the task. The restated lines cost about 70 tokens each and only appear when the rule ranks high.

### core/src/eval.rs · select_half, run_fused, run_dense

- refers-to: core/src/eval.rs :: run_fused
- kind: explainer
- logged: v0.10.2 · 2026-10-30

`--half even|odd` keeps gold lines by line-number parity (even is the tuning half). `run_fused` serves the formslot and dense arms: it calls `search::retrieve_with` with the arm's extra lists, then `pack::build_with` on the same retrieval, so the ranking and the pack come from one search (before, the search ran twice). `run_dense` exists only with `--features dense`: it embeds each task, queries `unit_vec` directly (the same SQL as `dense::vectors::nearest`, which is private to a file outside this change; making `nearest` public would remove the copy) and fuses the 100 nearest units at weight 1.0. A default build keeps the arm as not run with the reason. In this session the dense build compiled with no warnings, the model and tokenizer were fetched into a scratch copy and onnxruntime 1.31.1 came from the pip wheel, but embedding the 17,608 formvis units took 10 minutes for 1,152 units on two cores (about 2.5 hours in all), so the arm was not run.

### core/src/search/pack.rs · build_with (restate removed)

- refers-to: core/src/search/pack.rs :: build_with
- kind: removed-code
- logged: v0.10.2 · 2026-10-30

Removed: restating an owner rule that P0 already carries as a memory line in P2 (`RESTATE_WITHIN` 5, `RESTATE_MAX` 2). The review showed it raised R@4k from 0.871 to 0.912 with no change in R@4k+P0 (0.918 both ways): a metric movement, since the agent already had the rule in the prefix. Convention tasks went from 0.850 to 0.550 on R@4k, the other kinds did not move. Owner rules in P0 are now skipped in P2 whenever P0 is sent (a T1 pack has no P0, so there they still print). The `files:` line stays and does help (+0.061 on R@4k and on R@4k+P0), but it only lists paths: more candidate places to look, not content and not a better ranking. Identical memory records (one HASHOUT entry fans out to one record per anchor, with the same title and text) are shown once in a pack, keyed by `content_hash` over kind, authority, title and text.

### core/src/eval.rs · select_half, clusters

- refers-to: core/src/eval.rs :: select_half
- kind: rationale
- logged: v0.10.2 · 2026-10-30

The line-parity halves were not held out: gold is history ordered, so neighbours share files and commits, and parameters were chosen on all 147 tasks. `clusters` joins tasks that share any expected file, symbol or slot or the same 7-hex source commit (union-find, 49 clusters, largest 33), and `select_half` gives each whole cluster to the held-out half when the first byte of the SHA-256 of its smallest task id is odd. That is deterministic and cluster-separated, but unbalanced (114 tuning, 33 held-out, no convention or decline in held-out), and the gold set was written before tuning with some parameters picked on all tasks, so the held-out number is indicative only. Memory phrases are not link keys: owner rules are shared by many unrelated tasks.

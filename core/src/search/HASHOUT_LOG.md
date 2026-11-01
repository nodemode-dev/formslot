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

### core/src/search/docs.rs · units, mode_of

- refers-to: core/src/search/docs.rs :: units
- kind: rationale
- logged: v0.11.0 · 2026-10-30

The held-out gold showed that only files the symbol extractors parse were searchable: Markdown, JSON, TOML, YAML, shell and batch scripts and extensionless launchers (`formvis`, `setup/freeze_backend`) had no search units, so any task that named them (a plugin skill, a hooks.json, a launcher, a NOTICE row, a sub-skill) could only be answered through a slot or a memory record. `sync` now also reads files that have no parsed units, role source, doc or data, not binary, at most 256 KiB, and builds search units from them: Markdown is cut at headings of level 1 to 3 (kind `section`, qualified is the heading trail, sections under 24 bytes dropped, the path is the signature so directory words match), other config and script files become one `text` unit, and an extensionless file counts only with a shebang. HASHOUT and CHANGELOG logs (role log), vendored, mirror, generated and binary files and lockfiles are left out. The units reach the pack as unit lines and excerpts through the existing path (`unit_text` reads byte spans). They are excluded from the PageRank graph (`rank.rs`) so the P0 repo map does not fill with docs. Costs: formvis search units 17.6k to about 20k, median latency 259 ms to 460 ms on formvis, and the dense tier will embed the extra units (not changed here). A rebuild is needed (`formslot search rebuild`): unchanged blobs with no units are re-read on every sync for the extensionless candidates only.

### core/src/search/mod.rs · fuse_weighted, TEXT_PENALTY

- refers-to: core/src/search/mod.rs :: fuse_weighted
- kind: rationale
- logged: v0.11.0 · 2026-10-30

Text units (`section`, `text`) are multiplied by 0.8 in the fusion, as test files are by 0.5. Without it the doc sections flooded the top ranks of code tasks on the formvis gold (R@1 0.320 to 0.259, R@10 0.844 to 0.762 with no penalty). Tried on the tuning half and formvis gold: 0.6 (R@10 0.837 on formvis gold, formslot tuning R@10 0.688) and 0.8 (0.830, 0.750); 0.8 kept. It is a small penalty because documentation is the right answer for rule and config questions, and the memory records already carry the rules.

### core/src/search/lexical.rs · memory_notes, memory_by

- refers-to: core/src/search/lexical.rs :: memory_notes
- kind: rationale
- logged: v0.11.0 · 2026-10-30

Memory ingest now reads each corpus's own docs (see core/src/memory/HASHOUT_LOG.md), which added 1,972 derived how-to records to formvis alone. Left in the shared pool with the owner rules they pushed rules out of the 20-record pool (formvis gold R@1 was 0.259 and R@10 0.769 with them in the shared pool, 0.347 and 0.837 with their own). They get their own pool, `memory_notes`: derived how-to only, same 30 percent term coverage, at most three, weight 1.0 (owner rules and declines keep weight 2.0, hashout and tombstone 0.7). `memory_by(Some(false))` now excludes derived how-to. Effect on formvis gold with the docs ingested: R@4k 0.864 to 0.884, R@10 0.844 to 0.830, R@1 0.320 to 0.354.

### core/src/search/mod.rs · retrieve_with (limits seen)

- refers-to: core/src/search/mod.rs :: retrieve_with
- kind: warning-context
- logged: v0.11.0 · 2026-10-30

Not fixed by this change, seen on the tuning half: expected code that the task describes in words the code does not use stays low (h042 find_bin for "finds an executable on this machine" is not in the top 100; h106 dedup.rs at 59 for "near-copies in tiers"; h112 redact.rs at 77 for "scrub a key"; h039 dom.ts at 43). That is a vocabulary gap the lexical tiers cannot close; the dense tier is the intended fix. Slots from another repository also take the top five ranks of unrelated tasks (their text match alone is enough to enter at weight 2.0); not changed because the formvis tuning leans on it.

### core/src/search/index.rs · sync, add_docs

- refers-to: core/src/search/index.rs :: sync
- kind: rationale
- logged: v0.11.0 · 2026-10-30

The doc-unit query skips any file that has a `scan_finding` row for its corpus, so a file the secret scan flagged is never turned into searchable section or text units. `add_docs` returns nothing for a blob that is not valid UTF-8 and slices the body from the decoded text by the unit's byte range; lossy decoding changed byte lengths and made the stored offsets point at the wrong text.

### core/src/search/docs.rs · mode_of, heading_of, markdown, unique

- refers-to: core/src/search/docs.rs :: markdown
- kind: rationale
- logged: v0.11.0 · 2026-10-30

`mode_of` returns None under `docs/eval/` so the evaluation's own gold and reports cannot be retrieved as answers. A heading loses only a closing `\s+#+` sequence, so `## C#` stays `C#`. The open fence remembers its marker character and length: a `~~~` line inside a backtick fence no longer toggles, and only a bare marker at least as long closes it. Duplicate section trails are numbered from a HashMap counter instead of rescanning a Vec for each retry, which was quadratic on files with many equal headings.

### core/src/search/slots.rs · rank, grounded

- refers-to: core/src/search/slots.rs :: rank, Match
- kind: rationale
- logged: v0.13.0 · 2026-10-30

A slot from another repository took the top ranks of unrelated tasks at weight 2.0, because the best text match of a slot list always exists, however weak. A slot is now `grounded` when its origin repository is the searched one (or is empty), or when a unit of the pool sits in one of its files, or when the query terms found in its text carry at least 60 percent of the idf mass of the query terms that occur in any slot. Ungrounded slots enter the fusion at weight 0.5 in their own list; grounded ones stay at 2.0. Count coverage (40 percent) was tried first and dropped: the formvis toast slot is the right answer to a subform "show a failure notice" task on one weak word, and a count threshold demoted it. On the tuning halves, subform R@1 0.243 to 0.378 and R@4 0.432 to 0.649; gold.jsonl tuning kept R@10 (0.807 to 0.798) once own-repository slots were exempt (demoting those too cost formvis R@10 0.807 to 0.711). Coverage 0.6, 0.7 and 0.85 gave the same numbers; 0.6 was kept. The one case that loses: a foreign slot that is right on a short, paraphrased task falls to weight 0.5 unless a unit of the pool names one of its files.

### core/src/search/mod.rs · retrieve_with (text pool)

- refers-to: core/src/search/mod.rs :: retrieve_with, WEIGHT_TEXT
- kind: rationale
- logged: v0.13.0 · 2026-10-30

R@1 and R@4 had fallen on the subform and formvis tuning halves when doc units (`section`, `text`) came in, because they shared the lexical and trigram lists with code and pushed code units down a rank each. They now have their own lexical (40) and trigram (20) lists at weight 1.0, as the how-to notes got their own pool; code lists contain code only and feed the graph seeds. The 0.8 text penalty stays (0.6 and 0.9 gave the same or lower R@1; with no penalty subform tuning R@1 fell to 0.297; text weight 0.7 and 1.3 were no better, 1.3 was much worse). The slot file-evidence pool is the fused top ten of all four lists.

### core/src/search/lexical.rs · covered

- refers-to: core/src/search/lexical.rs :: covered, term_weights
- kind: rationale
- logged: v0.13.0 · 2026-10-30

Convention and decline records ranked below the pack because the 30 percent count coverage was taken over every query term, including function-like words and words that no record contains ("lucide", "vitest"). A record now enters when any of three holds: the share of query terms that occur in the memory index and are found in the record is at least 30 percent, the idf mass of the found terms is at least 30 percent of the mass of the indexed terms, or it holds one rare query term (idf at least 4.5 over all active records). The rule pool widened from 20 to 60 because bm25 puts long records first. Of the six named tasks (h054 h055 h079 h085 h087 h094, all in the held-out half, so none could drive the choice), h055 and h079 are now in the pack, h094 is at rank 7 but not in the pack, h054, h085 and h087 are still missing: their records share no more than one weak word with the task, a paraphrase gap the lexical tiers cannot close.

### core/src/search/index.rs · fts_insert (identifier and path words)

- refers-to: core/src/search/index.rs :: fts_insert, core/src/search/text.rs :: identifier_words, path_words, query_terms_cap
- kind: rationale
- logged: v0.13.0 · 2026-10-30

Vocabulary-gap code misses, by a general change only. Index side: the full-text row of a unit gets the split words of every compound identifier in its body (camelCase and digit boundaries; snake_case and kebab-case are already separated by the tokenizer), up to 2 KiB, and the split words of its path segments in the signature column. Query side: `query_terms` already split both styles; the lexical query now takes 24 terms instead of 12, because the 13th word of a long task ("trust" in h008) was dropped by position. No synonym list and nothing keyed on gold wording. Result on the five tasks: h008 not found to rank 53, h039 43 to 14 (now in the pack), h042 still not found, h106 61 to 66, h112 76 to 67. The index text changed, so `formslot search rebuild` is needed; trigram rows are unchanged.

### core/src/search/lexical.rs · term_weights (active df, idf floor)

- refers-to: core/src/search/lexical.rs :: term_weights
- kind: rationale
- logged: v0.13.0 · 2026-10-30

The idf used `total` over active records but `df` over the whole full-text table, which also holds stale, superseded and proposed rows, so df could exceed total and the log went negative, lowering the mass of common terms below zero. df is now counted over a join with `memory_record` where status is active, the same population as `total`, and idf is clamped at 0. A term found only in inactive records has df 0 and is skipped as before.

### core/src/search/pack.rs · build_p0 (repo map body cache), unit_text

- refers-to: core/src/search/pack.rs :: build_p0, MAP_BODY, unit_text
- kind: rationale
- logged: v0.14.0 · 2026-10-31

Profiled before any change (warm run, formvis, one task): the P0 repo map was the largest single stage of `pack` at about 200 ms, because `map::rank_entries` rebuilds the whole graph and runs 40 pagerank iterations for every pack, and its result is the same for every task. The rendered map body is now kept in a thread-local list of at most four entries, keyed by state that outlives a connection: the store file path, the corpus id, the corpus `head` and `indexed_at` read live from the store, and the corpus's `search_unit` and `edge` row counts. A reindex or search rebuild from another process changes at least one of these, so a long-lived process (`formslot mcp`, eval) builds the new map; two stores in one process never share an entry; the oldest entry is dropped past four. The first version keyed on `total_changes` and `data_version`, which are per connection and collided across the new Store opened for each MCP call; it was replaced after review. The text is byte-identical to the uncached one. A single CLI call still pays the build once. `unit_text` and the lexical statements use `prepare_cached`; rusqlite's `query_row` and `prepare` do not cache. Ranking code is untouched.

### core/src/store.rs · open (read-path pragmas)

- refers-to: core/src/store.rs :: Store::open
- kind: rationale
- logged: v0.14.0 · 2026-10-31

`mmap_size` 256 MiB, `cache_size` 64 MiB and `temp_store` MEMORY are set on every connection, and the prepared statement cache holds 64 statements instead of 16 so the per-pool search statements and the memory statements all stay cached. None of these changes results. No covering index migration was added: the full-text and trigram stages were 2 to 7 ms warm, so there was nothing for an index to win.

### core/src/search/pack.rs · stored repo map

- refers-to: core/src/search/pack.rs :: build_p0
- kind: rationale
- logged: v0.15.0 · 2026-11-01

A one-off CLI `pack` is a fresh process, so the thread-local map list never hit and the P0 repo map (about 200 ms of graph build and pagerank on formvis) was rebuilt every time. Migration 012 adds `map_cache`, one row per corpus holding the rendered P0 map body with the `head`, `indexed_at`, `search_unit` count and `edge` count it was built from, plus `rev`, the package version and a local revision digit. `build_p0` looks in the thread-local list first, then in the table (all six columns must equal the live state), and only then builds; a build writes the row and failures to write are ignored, so a read-only store still packs. The row is invalid when any key column differs, and a trigger on `corpus` deletes it when `head` or `indexed_at` is updated, which every reindex does. A corpus delete cascades. `MAP_REV` must be edited whenever `rank_entries`, `fit`, `params_of` or the P0 token budget change, otherwise a store written by the old code keeps serving the old map within one release; the package version covers release upgrades. The body is the same string the uncached path produced (the pack output and its P0 sha256 were compared on formslot-v140 and this build). Measured one-off `pack` on the subform corpus, 10 runs each: 147 ms median before, 96 ms median after the first run (the first run after the upgrade builds and stores, 153 ms).

### core/src/search/lexical.rs · covered (term weights once per query)

- refers-to: core/src/search/lexical.rs :: covered
- kind: rationale
- logged: v0.15.0 · 2026-11-01

`search::retrieve` calls `covered` for memory and again for notes with the same query, and each call ran `term_weights` (one `COUNT` plus one full-text count per term, about 19 ms warm, 38 ms for both). The weights now sit in a one-entry thread-local keyed by the store path, the query, and `memory_record` row count, active count, highest id, latest `updated_at` and the sum of the ids of active records (so one record retiring while another activates in the same second still changes the key), so the second call (and a repeated query in `eval` or `formslot mcp`) reuses them and any memory write that adds, retires or edits a record recomputes. The key read is one aggregate over an indexed-size table. The weights, the filter and its order are unchanged, so results are identical. The remaining double work, the slot manifest JSON being parsed again for every query in `slots::docs`, lives in slots.rs and is described in the v0.15.0 report for that file's owner. A warm profile of the rest of `pack` found no other single stage over 10 ms that this file set can remove: the lexical and trigram pools are 2 to 7 ms each, process start and migrate 4 ms.

### core/src/search/slots.rs · rank (rule slots, graded foreign weight)

- refers-to: core/src/search/slots.rs :: rank, is_rule_slot, Match::weight; core/src/search/mod.rs :: fuse_mixed, retrieve_with
- kind: rationale
- logged: v0.15.0 · 2026-10-30

The v0.13.0 foreign-slot demotion to 0.5 removed correct answers: `rule.family-laws` (present in every corpus, origin formvis), `ui.toast` and `ui.dialog-kit` are answers on weak matches in other repositories (h036 h038 h071 h085 h092 h095 h076 h080). Three changes. (1) A slot is a rule slot when its manifest kind is rule, its id starts with `rule.`, or all its files are a root CLAUDE.md, AGENTS.md or NOTICE.md. Rule slots are native everywhere: no repository test in the file evidence, and their file evidence reads the top 40 fused units, not the top 10. Weight is 2.0 when grounded (idf coverage at least 0.6, or a pool unit in a rule document) and 1.0 otherwise. (2) A foreign non-rule slot gets 0.5 + 1.5 * min(1, coverage / 0.6) where coverage is the idf-mass share of the query terms the slot covers (before: 2.0 at 0.6 or more, else 0.5), and 1.0 at least when it is the top slot and one of its files is among the top 40 fused units of any corpus. The weight rides on each slot hit (`Match.weight`) and `fuse_mixed` applies it per entry in one slot list ranked by slot order, cut at five; before there were two lists (grounded, weak) ranked separately. Measured alone, rule nativeness plus grading moved subform held-out R@1 and R@4 up .029 and nothing else; the deep file evidence alone moved nothing measurable. Gains are small because most named tasks fail on the unit list, not the slot weight.

### core/src/search/mod.rs · retrieve_with (doc unit trigram credit)

- refers-to: core/src/search/mod.rs :: retrieve_with, text_trigram
- kind: rationale
- logged: v0.15.0 · 2026-10-30

A doc unit gets one list entry (text lexical) where a code unit gets lexical, trigram and graph entries. The doc unit that tops the text lexical list now also enters the text trigram list at rank 1 when it is not there, with the 0.8 penalty kept. Effect: formslot held-out R@4 .412 to .471, subform held-out R@10 .543 to .571, gold.jsonl R@1 unchanged (.333 tuning, .394 held-out), gold.jsonl tuning R@4 .649 to .658 in one run; cost: ranks of 36 tasks moved down by 1 to 5 as the doc unit pushes code units, and h049 left the pack (v1 without credit has it in), subform held-out R@4k+P0 .857 to .829. Kept because R@1 and R@10 do not fall anywhere and the pack loss is one task; the owner can drop the block (about eight lines) if h049-type pack losses matter more.

### core/src/search/index.rs · fts_insert (ablation of identifier and path words, not adopted)

- refers-to: core/src/search/index.rs :: fts_insert; core/src/search/text.rs :: identifier_words, path_words
- kind: rationale
- logged: v0.15.0 · 2026-10-30

Built twice with the v0.13.0 index words off (query keeps 24 terms): both off, and body identifier words off with path words kept. Both off: h095 17 to 2, h071 20 to 3, h092 8 to 1, h076 9 to 3, h055 20 to 6, h094 6 to 2, h039 into the pack; but h049 stays out (39), h008 rank 52 to 61, and the tuning halves fall (subform R@1 .378 to .297, R@10 .784 to .703; formslot R@4 .625 to .500). Body words off only: tuning rises (subform R@4 .649 to .676, formslot R@4 .625 to .688), held-out R@4 formslot .471 to .529, but h008 52 to 72, h095 and h049 do not recover, and gold.jsonl held-out R@1 .394 to .333. The brief's condition (h049 and h095 recover without losing h008 and h039) is met by neither, so the index is unchanged and no rebuild is needed. The result is a split verdict for the owner: dropping body words is better on the tuning halves and worse on one gold held-out half.

### core/src/search/slots.rs · docs (per-process slot term table cache)

- refers-to: core/src/search/slots.rs :: docs, docs_key, build_docs, all, rank
- kind: rationale
- logged: v0.16.0 · 2026-11-01

`docs` rebuilt every slot's term table (manifest JSON to `Doc`, `split_words` and `stem` over id, title, tags, files, exports, summary, description) on every call, and `all` and `rank` can both run in one process (`retrieve`, `build_p0`, `formmap`). The built `Vec<Doc>` now sits in a one-entry thread-local behind an `Rc`, keyed by the store path, the slot row count, the highest `updated_at` and a hash over (id, status, version, content_hash, updated_at) of every slot row in id order. The key read touches five short columns and never the manifest JSON; status and version are in the hash so a status change within one second still invalidates. `build_docs` is the old body unchanged (canonical and proposed filter, term accumulation order), so output is identical. A store with no path (in-memory) skips the cache. Verified: `eval --json --arm formslot` on gold.jsonl formvis and gold_heldout.jsonl subform are identical to formslot-v150 apart from millis; the median per-task retrieve time fell from 89.6 to 61.5 ms (formvis) and 74.4 to 50.2 ms (subform held-out) on a loaded machine; and in a long-lived `formslot mcp`, a `slot_find` for a word that only a newly proposed slot contains returned no hit before `slot propose` from another process and the new slot after it. Limit: a manifest edited without a change of content_hash and updated_at would not invalidate, which no storage path does.

### core/src/search/mod.rs · retrieve_with (dense fusion)

- refers-to: core/src/search/mod.rs :: retrieve_with
- kind: rationale
- logged: v0.16.0 · 2026-11-01

The extra (dense) lists are no longer added to the global fusion. They are fused with the lexical and trigram code-unit lists first (weights 1, 1 and the dense weight), and that result takes the place of the two lists at weight 2.0, their combined weight; the second slot is emptied. Memory, rule, derived, notes, slot, graph and text lists are untouched, so dense can reorder code units among themselves but cannot add reciprocal-rank mass that outranks a memory or rule hit. Without extra lists nothing changes (default build and default ranking identical). Tried and dropped, on subform and formslot gold_heldout (105 tasks, both halves): plain weight 1.0 (v0.11.0), a weight cap of 0.4 and 0.2 when rule memory or notes were covered, and the same cap inside the pre-fusion. Convention R@1 .222 (no dense), .037 (plain and cap), .222 (pre); decline R@1 .125, .062, .125. Chosen: pre-fusion. Cap 0.4 had the best MRR on the tuning halves (.606 against .536) but did not repair convention and was worst on the held-out formslot half. Held-out R@1 stays at or below the no-dense arm (subform .286 to .257, formslot .235 to .118), so dense stays opt-in. Report: docs/eval/report_v0.16.0_dense.md.

The slot cache key also hashes `length(manifest)` per row (v0.16.0 review): a save of the same slot within one second that changed only the title, summary or tags kept the same version, hash and stamp, and a long-lived process served the old table until the next write.

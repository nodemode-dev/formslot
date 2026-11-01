# formslot evaluation · v0.11.0 · held-out set

Written: 2026-10-30 · Sets: `gold.jsonl` (147 tasks, formvis) and `gold_heldout.jsonl` (118 tasks: subform 72, formslot 33, formvis 13) · Store: a copy rebuilt with `search rebuild`, `memory ingest` per corpus and `slot refresh --all --corpus formvis`.

## Why this report exists

The v0.10.2 report measured recall on formvis only, the corpus the ranking was tuned on. The held-out set was written without sight of the ranking code. Measured with the v0.10.2 binary it showed that recall did not carry over: R@4k 0.653 on subform and 0.606 on formslot, against 0.864 on formvis.

## Diagnosis of the 39 v0.10.2 misses (R@4k)

By corpus: subform 25, formslot 13, formvis 1. The cause was not a role filter: no extractor produced search units for those file kinds.

| class | n |
| --- | --- |
| Expected file is Markdown, JSON or a script with no search unit, and its memory phrase was never ingested | 11 |
| Memory phrase in the store but ranked below the pack, and the expected file not searchable | 15 |
| Memory record ranked 1 to 5 but already in the stable prefix P0 (counted only by R@4k+P0) | 5 |
| Expected files are plugin Markdown, hooks.json, extensionless launchers or scripts | 3 |
| Expected code indexed but ranked low (vocabulary gap) | 5 |

Changes were chosen from tuning-half failures only. The held-out half and `gold.jsonl` are the check.

## Changes

1. Search units for files with no parsed symbols: Markdown by heading section to level 3, JSON, TOML, YAML, shell and batch files, shebang scripts, up to 256 KiB, not log, vendored, mirror, generated, binary or lockfile. Kept out of the repo map.
2. A 0.8 fusion penalty on those units (0.6 and 0.8 tried; 0.8 kept).
3. Memory ingest of each repo's own docs: Markdown under `.claude/skills/` and `docs/` (not `docs/eval/`) and NOTICE table rows, as derived how-to records with no triggers. Owner rules are unchanged.
4. A separate retrieval pool for those notes (at most 3, 30 percent term coverage).

No slot, lexical, trigram or graph weight was changed. No change keys on gold wording.

## Before and after

Columns: n · R@1 · R@4 · R@10 · MRR · R@4k · R@4k+P0.

| set | v0.10.2 | v0.11.0 |
| --- | --- | --- |
| gold.jsonl formvis | 147 · .320 · .592 · .844 · .478 · .864 · .912 | 147 · .354 · .619 · .830 · .502 · .884 · .918 |
| held-out subform | 72 · .236 · .389 · .569 · .329 · .653 · .694 | 72 · .250 · .389 · .653 · .364 · .903 · .917 |
| held-out formslot | 33 · .273 · .455 · .545 · .361 · .606 · .727 | 33 · .242 · .455 · .697 · .374 · .848 · .848 |
| held-out formvis | 13 · .231 · .385 · .692 · .328 · .923 · .923 | 13 · .154 · .385 · .615 · .265 · 1.000 · 1.000 |
| subform tuning half | 37 · .297 · .486 · .730 · .420 · .892 · .892 | 37 · .243 · .405 · .757 · .386 · .946 · .946 |
| subform held-out half | 35 · .171 · .286 · .400 · .233 · .400 · .486 | 35 · .257 · .371 · .543 · .341 · .857 · .886 |
| formslot tuning half | 16 · .188 · .438 · .562 · .319 · .812 · .812 | 16 · .188 · .375 · .750 · .321 · .875 · .875 |
| formslot held-out half | 17 · .353 · .471 · .529 · .400 · .412 · .647 | 17 · .294 · .529 · .647 · .424 · .824 · .824 |
| formvis tuning half | 6 · .167 · .500 · .500 · .277 · .833 · .833 | 6 · .000 · .500 · .500 · .154 · 1.000 · 1.000 |
| formvis held-out half | 7 · .286 · .286 · .857 · .372 · 1.000 · 1.000 | 7 · .286 · .286 · .714 · .359 · 1.000 · 1.000 |

## Reading it plainly

- The held-out halves improved: R@4k 0.400 to 0.857 on subform and 0.412 to 0.824 on formslot. Those halves hold most of the convention and decline tasks, so they mostly measure memory and doc coverage.
- The R@4k gain is coverage, not ranking. R@1 and R@4 fell on the subform tuning half (.297 to .243, .486 to .405) and on the formvis tuning half (n 6).
- formvis held-out lost one task each on R@1 and R@10 (n 13). That is not an improvement.
- `gold.jsonl` did not regress on the headline (R@4k .864 to .884); R@10 is one point lower.
- Median latency on formvis rose from 259 ms to 460 ms, from about 15 percent more search units.
- The tuning-half numbers are not independent, since that half picked the 0.8 penalty and the notes pool.
- 12 misses remain: 5 vocabulary-gap code tasks (h008, h039, h042, h106, h112), 6 convention or decline tasks whose record ranks below the pack, and h082, whose phrase lives only in formMCP's `data/declines.json`. Ingesting that file as formslot memory is the owner's decision.
- Slots from other repositories take the top ranks of some unrelated tasks at weight 2.0. Observed, not changed.

## Upgrading

A `formslot search rebuild` and a `formslot memory ingest` per corpus are needed to pick up the new units and records.

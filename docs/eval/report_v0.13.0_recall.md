# formslot evaluation · v0.13.0 · ranking and memory

Written: 2026-10-30 · Sets: `gold.jsonl` (147, formvis) and `gold_heldout.jsonl` (subform 72, formslot 33, formvis 13) · Store: v0.12.0 store copy, `search rebuild` and `memory ingest` per corpus. Changes were chosen on the tuning halves only.

## Changes

- A slot from a repository other than the searched one enters the fusion at weight 0.5 instead of 2.0, unless a unit of the pool is in one of its files or its text holds at least 60 percent of the idf mass of the query terms any slot contains. Slots of the searched repository are exempt.
- `section` and `text` doc units have their own lexical (40) and trigram (20) lists at weight 1.0 with the 0.8 penalty kept; code lists hold code only.
- Memory entry coverage is taken over the query terms the memory index contains at all; a record with one rare query term (idf at least 4.5) also enters; the rule pool is 60 instead of 20.
- The index adds the split words of compound identifiers in unit bodies (2 KiB cap) and of path segments; the lexical query keeps 24 terms instead of 12. No synonym lists, nothing keyed on gold wording.
- Memory: owner rules come only from the root CLAUDE.md, the root AGENTS.md and the skill laws; a nested CLAUDE.md or AGENTS.md, and NOTICE.md at any depth, give trigger-free how-to records. Each Markdown file gives at most 400 doc records and 128 KiB. An unchanged corpus skips ingest by fingerprint.

## Before and after

Columns: n · R@1 · R@4 · R@10 · MRR · R@4k · R@4k+P0 · median ms.

| set · half | v0.12.0 | v0.13.0 |
| --- | --- | --- |
| subform · tuning | 37 · .243 · .432 · .757 · .388 · .946 · .946 · 152 | 37 · .378 · .649 · .838 · .513 · .973 · .973 · 121 |
| subform · held-out | 35 · .257 · .343 · .571 · .345 · .800 · .886 · 134 | 35 · .286 · .429 · .486 · .379 · .800 · .857 · 109 |
| subform · all | 72 · .250 · .389 · .667 · .367 · .875 · .917 · 142 | 72 · .333 · .542 · .667 · .448 · .889 · .917 · 114 |
| formslot · tuning | 16 · .188 · .375 · .750 · .320 · .875 · .875 · 126 | 16 · .250 · .562 · .750 · .425 · .875 · .875 · 91 |
| formslot · held-out | 17 · .294 · .529 · .647 · .424 · .824 · .882 · 114 | 17 · .294 · .412 · .647 · .395 · .824 · .882 · 87 |
| formslot · all | 33 · .242 · .455 · .697 · .374 · .848 · .879 · 118 | 33 · .273 · .485 · .697 · .409 · .848 · .879 · 95 |
| formvis (held-out set) · all | 13 · .154 · .385 · .615 · .265 · 1.000 · 1.000 · 318 | 13 · .154 · .385 · .692 · .284 · 1.000 · 1.000 · 303 |
| gold.jsonl formvis · tuning | 114 · .351 · .632 · .807 · .501 · .868 · .912 · 290 | 114 · .342 · .640 · .798 · .499 · .886 · .930 · 264 |
| gold.jsonl formvis · held-out | 33 · .364 · .576 · .909 · .504 · .939 · .939 · 291 | 33 · .394 · .576 · .909 · .531 · .939 · .939 · 267 |
| gold.jsonl formvis · all | 147 · .354 · .619 · .830 · .502 · .884 · .918 · 304 | 147 · .354 · .626 · .823 · .506 · .898 · .932 · 269 |

Of the eleven named misses, three now reach the pack (h055, h079, h039); h085 got worse; h054, h087, h042, h106 and h112 share at most one weak word with their answer and stay out of reach of the lexical tiers.

## Reading it plainly

- The tuning-half gains are on the data they were chosen on. The held-out halves do not confirm them: subform held-out R@10 fell from .571 to .486 and R@4k+P0 from .886 to .857, and formslot held-out R@4 fell from .529 to .412. Subform held-out R@1 and R@4 rose.
- Demoting slots from other repositories helps tasks whose answer is native code and hurts tasks whose answer is only a ported file.
- `gold.jsonl` R@4k rose from .884 to .898; its tuning half lost a little R@1 and R@10, within noise.
- The named convention and decline tasks all sit in the held-out half; their mechanisms were diagnosed there (a mild leak), but no constant was tuned on them.
- Latency fell, partly noise on a shared 2-core machine.
- Unchanged re-ingest: 0.78 s to 0.026 s on formvis, with identical counts.
- Owner rules: formvis active rules 44 to 41 (three root NOTICE.md rules became how-to); subform active rules 47 to 31 (template files nested under backend/app/templates are no longer owner rules). Nothing is deleted.

## Upgrading

Run `formslot search rebuild` and `formslot memory ingest <corpus>` for each corpus.

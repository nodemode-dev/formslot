# Gold set for code retrieval over formvis

`gold.jsonl` holds 147 questions, one JSON object per line, in the shape of CONTRACT_T6 section A. The corpus is formvis at the indexed HEAD (`/home/claude/ref/formvis`, 1577 commits).

## How it was built

The author worked from the outside: only formvis (git history, files, CLAUDE.md, AGENTS.md, the harness SKILL.md), the formslot slot manifests (for slot ids and their file lists) and `data/declines.json` (for the owner's declined features). No formslot source, search or ranking code, logs or contract search section was read.

Each question is a request a developer could put to an agent before the change existed. Task wording is natural language; symbol and file names are not copied in unless a developer would say them (for example the button names in the UI).

## Kinds and sources

| kind | count | share | where the question came from |
| --- | --- | --- | --- |
| find | 67 | 46% | A real feat or fix commit. The task describes the missing feature or the bug; the expected files are the ones the commit touched that still exist at HEAD, plus a symbol when the commit centred on one. `source` is `commit <sha> <subject>`. |
| reuse | 47 | 32% | A shared helper that exists at HEAD (row builders in `controls.ts`, DOM helpers in `dom.ts`, dialog kit, toasts, `i18n`, `api.ts`, backend `outbound`, `durable`, `secrets`). Where the code is duplicated, both copies are accepted (`hexRgb` in `develop.ts` and `dither.ts`, `srgbToLinear` in `deep.ts` and `relight.ts`). `source` is `path:line` of the definition. |
| convention | 20 | 14% | A rule in CLAUDE.md, AGENTS.md or the harness SKILL.md laws (commits, layering, NOTICE rows, tokens, version bump, licences). `memory` holds a short phrase lifted from the rule; `source` is the file and line. |
| decline | 13 | 9% | A request the owner declined (seam carving, skin-tone protection, Poisson blending, live fill during Move, other providers, sparkle icon, Adobe feature names, masks that limit the fill, the old product name, face meshes, copied vendor help). `memory` phrases are the `find` phrases from `declines.json`, or a sentence fragment from CLAUDE.md. `source` is the CLAUDE.md line. |

## Fields

- `expect.files` and `expect.symbols` exist at HEAD (checked with `git ls-files`; every symbol checked by a definition regex in the named file).
- `expect.slots` is filled automatically: every slot whose manifest lists any expected file. 83 of 147 lines name a slot; the rest touch code outside the 27 slot manifests (engine, tools, backend, Tauri).
- `expect.memory` is non-empty only for convention and decline lines.

## Known weaknesses

- Skew to recent history: most commit sources sit in 1.55 to 1.98 because older release commits are bulk or docs-only and make poor single-feature questions.
- Find lines often have no symbol (56 of 147 lines have none): a large commit touches many functions and naming one would be arbitrary. Scored on files only.
- Some expected files are very large, mixed-purpose files (`frontend/src/app.ts`, `engine/index.ts`); a retriever that lands on a neighbour is penalised harshly.
- A commit may have touched more files than listed. Only files still present and central to the described change are expected, so an answer that finds a valid extra file is scored as noise.
- Convention and decline phrases are copied from the owner's text; a memory store that paraphrases them may miss on exact match.
- The 27 slot manifests are formvis-family UI and shell code, so reuse questions on backend or engine helpers have no slot to hit.
- One author, one pass: no second reviewer checked that each task is unambiguous. A few reuse tasks (for example the `hexOf` one) have several plausible helpers across files; those list every copy found, but a missed copy would be marked wrong.
- The set measures finding, not whether the proposed edit is right.

## Held-out set (v0.11.0)

`gold_heldout.jsonl` holds 118 questions in the same JSON shape (`id`, `corpus`, `task`, `kind`, `expect.slots`, `expect.files`, `expect.symbols`, `expect.memory`, `source`). Ids are `h001` and up. Ids are stable labels, not a count: 16 drafted lines were dropped after validation to stay under 120, so the sequence has gaps.

### How it was written

- Written by a fresh-context author (agent `fs-gold2`) who never saw the ranking code. Not read: `core/src/search/`, `core/src/eval.rs`, `core/src/memory/`, and the existing evaluation reports. Read: this file, the first ten lines of `gold.jsonl` (for the shape), the slot manifests in `data/slots/` (for slot ids and file lists), `data/declines.json`, the git history of the three corpora, and the owner-rule documents named in each `source`.
- Tasks come from real history. `find` lines come from feature and fix commits (`source` is `commit <sha> <subject>`); the expected files are the ones the commit touched that still exist at HEAD, with a symbol when the commit centred on one. `reuse` lines come from a helper that exists at HEAD (`source` is `path:line` or a path). `convention` and `decline` lines come from rules in each repository's CLAUDE.md, AGENTS.md, harness SKILL.md and sub-skills, NOTICE.md and docs/MCP.md, and from `data/declines.json`; `memory` holds a lowercase phrase lifted from the rule.
- Wording is how an agent or developer would ask before the change exists. File and symbol names appear only where a person would say them.
- Aimed at no overlap with `gold.jsonl`: no task text is shared. Formvis lines avoid the commits and helpers of the first set; two of them (`h118`, `h123`) still list a file the first set also expects (`prefs.ts`, `history.ts`). The subform and formslot corpora are new to the evaluation. Several subform reuse lines name helpers that were ported from formvis and exist in both corpora (`dom.ts`, `dialogs.ts`, `toast.ts`, `controls.ts`).
- Checked by a read-only script: every line parses, has the required fields, every expected file is in `git ls-files` of its corpus at HEAD, every symbol matches a definition regex in its file, ids are unique, memory is non-empty exactly for convention and decline lines, and `expect.slots` equals the slots whose manifests list an expected file (for formslot lines `rule.family-laws` is also given where the expected file is CLAUDE.md).

### Counts

| kind | count |
| --- | --- |
| find | 53 |
| reuse | 22 |
| convention | 27 |
| decline | 16 |

| corpus | count | find | reuse | convention | decline |
| --- | --- | --- | --- | --- | --- |
| subform | 72 | 30 | 14 | 19 | 9 |
| formslot | 33 | 14 | 4 | 8 | 7 |
| formvis | 13 | 9 | 4 | 0 | 0 |

The corpus id `subform` keeps the editor's earlier name: the editor is now FormSubstrate (the app Substrate), and the sets and reports are left as they were measured.

### Weaknesses specific to this set

- The corpus label `formslot` for convention and decline lines drawn from `data/declines.json` (two formMCP declines, `h081` and `h082`) is a convenience: those rules belong to formMCP and live in formslot's data folder.
- Convention and decline tasks for subform and formslot all point at a handful of files (CLAUDE.md, NOTICE.md, the harness sub-skills), so file recall is easy and memory recall is the real test. The `memory` phrases are exact lowercase fragments of the owner text and a paraphrasing store may miss them.
- No expected file or symbol sits in `core/src/search/`, `core/src/memory/` or `core/src/eval.rs`, so nothing in the set was chosen from code the author was barred from reading; as a result the set does not test finding the ranking, memory or eval code itself.
- Reuse lines whose helper has a twin (`h045` minimal_env in two files, `h129` homography in two files) list every copy found; a missed copy would score wrong.
- Mixed skew: subform history is only about 100 commits, so several `find` lines come from one commit (for example `abcd59a`, `0eb05a7`).

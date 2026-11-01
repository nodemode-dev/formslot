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

---
name: formslot-dev
description: Work on the formslot repository itself, the Rust codebase memory service for coding agents. Use when changing or reviewing code under core/src or cli/src, adding a command, an MCP tool, a hook event, a store migration or a search list, touching the release scripts or the check workflow, running formslot eval, bumping the version, or preparing a hand-back for nodemode-dev/formslot. Maps the two crates, the store migrations, the MCP and hook surfaces, the evaluation workflow and the release path, and states the rules that bind every change. Not for using formslot in another repository; that is the formslot-lookup skill.
---

# formslot development

Read `CLAUDE.md` first: it holds the owner's rules. This skill is the map that goes with them. The contract every surface must keep is `docs/CONTRACT.md`; the reasons behind the code are in the `HASHOUT_LOG.md` beside each folder. Read the log beside a file before you edit the file, and add an entry in the same change.

## Rules that bind every change

- Edit by hand, one region at a time, after reading the file. No sed, no patch, no script that changes tracked files. A file that is generated (`agents/manifest.json`) comes only from its generator, `formslot agents sync`.
- No test is written until the owner asks in words. What should be tested goes into `TEST_PLAN.md` in the planning sidecar, never into this tree.
- Nothing you run may start a real agent program. `formslot eval --arm model` starts `claude`, and `binres::find` looks for `claude`, `codex`, `cursor-agent` and `agy`. Run every command with a made-up `HOME`, a made-up `FORMSLOT_DATA_DIR`, PATH held to the system folders, and no `FORMSLOT_CLAUDE_BIN`.
- The engine owns the logic. `cli/src` parses arguments, calls `formslot_core` and prints. A new behaviour goes into `core/src` and is reached by the command, the MCP tool and the HTTP route alike.
- The service listens on 127.0.0.1 and checks Host, Origin and the token on every request (`core/src/http.rs`). Do not add a route that skips `handle`.
- Every new crate, program, model or data file gets a row in `NOTICE.md` before it is used. GPL stays outside the bundle, AGPL nowhere.
- No emoji anywhere, no em dash in Markdown, near-zero comments in code. Messages to people are plain English sentences.
- Never type a count. The tool count comes from `core/src/mcp/schema.rs` (`formslot agents sync` prints it), the gold set size from `docs/eval/gold.jsonl`, the slot count from `data/slots/`.

## Where things are

| Area | Files | What lives there |
| --- | --- | --- |
| Command line | `cli/src/main.rs`, `cli/src/*_cmd.rs` | the help text, one file per command group; thin |
| Store | `core/src/store.rs`, `core/src/migrations/001` to `012` | SQLite through rusqlite; migrations run in order on open |
| Indexing | `core/src/corpus.rs`, `git.rs`, `symbols/`, `hygiene.rs` | reads git objects at a commit, extracts units with tree-sitter, scans for secrets |
| Search and packs | `core/src/search/` | lexical and trigram lists, fusion, the repo map, the pack and token estimates |
| Slots | `core/src/slots/` | manifests, storage, copies, suggest, refresh, the consumer lockfile and drift |
| Memory | `core/src/memory/` | ingest of rules, declines and HASHOUT entries, recall, proposals |
| MCP | `core/src/mcp/` | `mod.rs` (both protocol eras), `schema.rs` (tool definitions), `tools.rs` and `curate.rs` (tool bodies), `http.rs`, `stdio.rs`, resources, prompts, paging |
| Hooks | `core/src/hook.rs`, `check.rs`, `install.rs`, `agentfiles.rs`, `connect.rs` | the Claude Code events, the write check, merging into a project, the skill and agent files, the other agent programs |
| Service | `core/src/http.rs`, `api.rs`, `lifecycle.rs`, `discovery.rs` | the local HTTP service, its routes, idle shutdown, the discovery file |
| Setup | `core/src/setup.rs`, `cli/src/setup_cmd.rs` | the guided setup and its undo |
| Keys | `core/src/keys.rs`, `count.rs` | the Anthropic key and the one remote call that uses it |
| Evaluation | `core/src/eval.rs`, `eval_model.rs`, `docs/eval/` | retrieval measures over the gold sets; the model arm |
| Plugin | `plugin/formslot/` | the Claude Code plugin: hooks, skills, the scout agent, the server entry |
| Release | `setup/build_release`, `setup/build_release.bat`, `.github/workflows/ci.yml` | what formcompiler runs, and the check it waits for |

## Adding things

- A command: a function in the matching `cli/src/*_cmd.rs`, an arm in `main.rs`, a line in `HELP`, a row in the command table of `docs/CONTRACT.md`.
- An MCP tool: a definition in `schema.rs` with a title, an input schema with `additionalProperties: false` and annotations; a body in `tools.rs` or `curate.rs` that returns through `page::text_result`; an arm in `tools::dispatch`; a row in the contract's tool table. Then run `formslot agents sync` and commit `agents/manifest.json`. A tool that returns large text returns content blocks only: Claude Code passes `structuredContent` to the model instead of the blocks when both exist.
- A migration: the next numbered file in `core/src/migrations/`, registered in `store.rs`. Never edit a migration that has shipped; a store written by any released version must still open.
- A hook change: `plugin/formslot/hooks/hooks.json` and `event_groups` in `core/src/install.rs` must say the same thing. An earlier matcher that install once wrote goes into `EARLIER_MATCHERS` so a reinstall updates it.
- A skill or agent that install should bring: a part in `core/src/agentfiles.rs`, read with `include_str!` from `plugin/formslot/`.

## Building and checking

- `cargo build --locked` for a quick build; `./formslot_oc_install` or `./formslot version` for the release build the launcher runs.
- `cargo test --locked` compiles both crates. There are no tests yet, by the owner's rule.
- `setup/build_release` stages `dist/formslot-vX.Y.Z-OS-ARCH/` with the program, the plugin, `data/`, LICENSE, NOTICE and README, and writes the archive and its checksums. On macOS `--dmg` also packs a disk image; on Windows `setup\build_release.bat --no-sums` is what formcompiler runs.
- `setup/package_skill [FOLDER]` runs the skill gates on one skill folder and packs it.
- `formslot agents sync --check` fails while `agents/manifest.json` differs from the definitions.
- A change to anything that crosses systems is type-checked for the Windows and macOS targets with `cargo check --locked --target ...` before it is handed over, or the hand-back says "Not run".

## Evaluation

`formslot eval` measures retrieval and changes no ranking. It reads `docs/eval/gold.jsonl` (and `gold_heldout.jsonl`), needs the named corpora indexed, and prints recall, MRR, nDCG, tokens and latency for the `formslot`, `baseline` and `dense` arms. `--half tuning` and `--half held-out` split the set; tune on the first and report on the second. `--out FILE` writes the Markdown report that goes into `docs/eval/`. `docs/eval/GOLD.md` says how the sets were written. `--arm model` is the only arm that calls a model and the only command that starts `claude`: do not run it unless the owner asks.

## Versions and hand-back

- The version is written in five places and all move in the release commit, which is last on the branch: `Cargo.toml`, the two formslot entries in `Cargo.lock`, the README badge and `plugin/formslot/.claude-plugin/plugin.json`. `CHANGELOG.md` gets a section in the same commit.
- Commits have one author and committer, `nodemode-ai <mackin@nodemode.ai>`, and no attribution line of any kind. One commit per coherent change, in the style of `git log`.
- Work goes back as a zip of the repository on a working branch with a clean tree, the pull request text, and the planning sidecar. Never push, never create a `compile_v*` branch.
- formcompiler builds the version it finds on `main` once `ci.yml` has passed on that commit. Do not change the README status line.

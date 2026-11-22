# Changelog

Versions are written `vX.Y.Z` with dots. Dates are US Eastern. Each section is written from `git log`; the reasons behind a change are in the `HASHOUT_LOG.md` beside the code. This file starts at 0.16.0; earlier versions are in the git history.

## 0.18.2 · 2026-11-22

- This changelog. Every release commit from here on adds a section.
- The development skill and the README say that a release adds its changelog section.

## 0.18.1 · 2026-11-22

- `formslot eval --arm model` exits 1 on Windows before anything starts and says so. On Windows it could not stop a `claude` run that passed its time limit, or make its exported copy of the project read-only. The other arms run on Windows.
- `setup/build_release` stops on macOS when the program was built with the dense feature, which a signed macOS release cannot load.
- The guide's macOS line leads with opening the signed disk image. CLAUDE.md names the family list formslot reads, formMCP's `family.json`.

## 0.18.0 · 2026-11-22

- New `formslot connect codex|cursor|antigravity [<repo>] [--user] [--remove] [--dry-run]`: adds the formslot MCP server to that agent program's own configuration and changes nothing else in the file. The README has a table of which program gets what. Hooks, skills and the scout agent are Claude Code's only.
- `formslot doctor` and `formslot setup` look for all four agent programs: `claude`, `codex`, `cursor-agent` and `agy`.
- The hook matchers follow Claude Code's current events and tools: a forked session gets the digest, and a notebook write is checked. Run `formslot install <repo>` again to update a project connected earlier; a matcher you changed yourself is left alone.
- `formslot hook` no longer lists PreCompact, which did nothing.
- The lookup skill names `slot_status` first and the new tools. New `formslot-curation` skill for the owner: from `slot suggest --write DIR` to a confirmed slot. `formslot install` brings it.
- The scout agent also calls `slot_status` and `slot_recall`, and never a tool that writes.
- For people working on formslot: a development skill in `.claude/skills/formslot-dev`, `setup/package_skill` takes a skill folder, and a `validate` workflow runs the official skill and plugin validators.

## 0.17.0 · 2026-11-22

- The MCP server speaks revision 2026-07-28, which has no handshake, beside the handshake revisions back to 2024-11-05, on stdio and on `POST /mcp`. `server/discover` is answered.
- An `initialize` with a version the server does not know is answered with 2025-11-25, the newest handshake revision, where it was 2025-06-18.
- `POST /mcp` reads `MCP-Protocol-Version`: an unsupported value is 400.
- `slot_source` and `slot_context_pack` declare a result size of 200,000 characters, so Claude Code keeps their text in the conversation instead of writing it to a file.
- Four new tools, 13 in all: `slot_recall`, `slot_remember` (stores a proposal for the owner; cannot make a rule), `slot_suggest` and `slot_drift`. Each returns the same JSON as its command.
- New `formslot agents sync [--check]` writes `agents/manifest.json`, the list of tools with their classes and the skills. The check workflow fails while it differs.

## 0.16.6 · 2026-11-22

- `formslot install <repo>` also writes the `formslot-lookup` skill and the `formslot-scout` agent into the project, where Claude Code loads them. It updates its own earlier text and leaves a file you changed. `formslot doctor <repo>` reports both files, and the guided undo removes what install wrote.
- A marketplace file, so the plugin can also be installed with `claude plugin marketplace add nodemode-dev/formslot` and `claude plugin install formslot@formslot`. Use one route per project.
- The README's release links open this repository's releases.

## 0.16.5 · 2026-11-22

- The guide, the README and the contract name the three commands that send data and what each sends: `pack --count remote`, `dense fetch --yes` and `eval --arm model`. Nothing else leaves the computer.
- New `formslot key set`, `key remove` and `key status`: the Anthropic key is kept in the system password store, or in a file only you can read where the computer has none. Status never reads the password store.
- New `SECURITY.md`. NOTICE lists `claude` among the programs found on the system.

## 0.16.4 · 2026-11-22

- `formslot.bat` runs a built program when cargo is not on PATH, as `formslot` does.
- Both launchers build and run in `CARGO_TARGET_DIR` when it is set, where the installers already built.
- `.gitignore` lists only what this repository makes.

## 0.16.3 · 2026-11-22

- A check workflow, `ci.yml`, that formcompiler waits for before it builds a commit.
- `setup\build_release.bat --no-sums` writes no checksum file that signing would make wrong.
- `setup/build_release --dmg` signs the program when a signing identity is set and packs a disk image the notary takes.

## 0.16.2 · 2026-11-22

- The README follows the family template, with screenshots, icons and app names (5 November).
- The editor formslot works with is named FormSubstrate, and the application is Substrate. The evaluation corpus id keeps its earlier name.

No commit on `main` carries 0.16.1: the history goes from 0.16.0 to 0.16.2.

## 0.16.0 · 2026-11-01

- `formslot slot suggest` proposes new slots from duplicate clusters the store already found. It keeps drafts already in the folder and cannot clash on long ids.
- The dense list reorders code units only, so it no longer pushes owner rules and declines out of the results.
- `slot_context_pack` reaches the model as text, `slot_conventions` keeps owner rules, and structured results carry more.
- The slot term table is cached per process and rebuilt when a slot changes, also when a manifest is saved within the same second.
- The contract records the MCP result shapes that reach Claude Code.

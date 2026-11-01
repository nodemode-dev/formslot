<h1 align="center">formslot</h1>

<p align="center"><b>The Form family's memory for coding agents: what is already built, where it lives, and the rules it follows, served in as few tokens as the task needs.</b></p>

<p align="center">
  <img src="https://img.shields.io/badge/version-v0.11.0-f97316" alt="version v0.11.0" />
  <img src="https://img.shields.io/badge/status-release-8b5cf6" alt="status release" />
  <img src="https://img.shields.io/badge/license-GPL--3.0-555555" alt="license GPL 3.0" />
</p>

formslot indexes every Form app's repository at exact commits into one deduplicated store of symbols, canonical slots (reusable components and features with their provenance and copies) and owner memory (rules, decisions, declined approaches). Agents reach it over MCP and Claude Code hooks, so they find and reuse what exists instead of re-reading or re-writing it. Its window is subform, the family's developer workbench.

**Status:** v0.11.0. v0.11.0 adds a model arm that measures pass rate through Claude Code (formslot 100 percent against 96.7 for the baseline on 30 formvis tasks, with 60 percent fewer input tokens), a held-out gold set from three repositories, search units and memory for Markdown, config and script files (held-out R@4k 0.653 to 0.903 on subform, 0.606 to 0.848 on formslot), and the first full dense run, which stays opt-in. Phases S1 to S11 have landed, S11 being the service lifecycle (start on demand, leases, idle shutdown, `stop`, `doctor`) and packaging (`setup/build_release`, `formslot_oc_install --user`, the lookup skill zip). Earlier phases: the service on 127.0.0.1 with its discovery file and access checks; indexing of git repositories at a commit (file manifest, languages, roles, licences, secret findings, incremental re-index); symbols and a per-repository graph from tree-sitter (TypeScript, Python, Rust, CSS, HTML); duplicate detection in three tiers plus a related band across repositories, with drift between two; and slots: manifests with provenance, hashes and copies, 26 proposed first-load slots in `data/slots/`, owner confirmation, and a `slots.lock.json` with per-file drift for consuming repositories; memory records (owner rules, declined approaches, HASHOUT entries and decisions, with recall, proposals and anchor checks); and retrieval: bm25 and trigram search fused with the graph, memory and slots, a PageRank repo map and FORMMAP.md, and token-budgeted context packs with a byte-stable prefix; an MCP server with eight `slot_*` tools over stdio (`formslot mcp`) and HTTP; and Claude Code hooks and a plugin (`plugin/formslot/`): a map digest at session start, a reuse and declined-feature check before every write, and a duplicate audit at stop, installed into a repository with `formslot install`; and evaluation: `formslot eval` over a 147-task gold set (`docs/eval/`), with an optional dense tier behind the `dense` cargo feature. Packaging and the service lifecycle arrive in later phases; the plan of record ships in the planning sidecar zip. The contract clients build against is `docs/CONTRACT.md`.

**Stack:** Rust (`formslot-core`, `formslot-cli`), SQLite bundled through rusqlite, the system git. Rust 1.90 or later.

## Quick start

```bash
./formslot_oc_install                 # checks git and cargo, builds the release binary
./formslot add ~/path/to/a/form/repo  # index it at HEAD
./formslot stats <name>               # files, lines and bytes by language, roles, licences
./formslot symbols <name> --count     # extracted units
./formslot refs <name> revealApp      # who uses a symbol
./formslot dupes <name>               # clone groups and near duplicates
./formslot dupes <name> --related     # same purpose, different bodies
./formslot slot seed data/slots       # load the proposed first-load slots
./formslot slot refresh --all --corpus <name>
./formslot slot get ui.control-rows   # manifest, files, copies, events
./formslot slot drift --repo <path> --infer   # vendored copies against their slots
./formslot dupes --across <a> <b>     # code shared between two repositories
./formslot memory ingest <name>       # owner rules, declines, HASHOUT entries, decisions
./formslot recall "seam carving"      # ranked memory records with a token estimate
./formslot search "colour row" --corpus <name>
./formslot map <name> --formmap       # FORMMAP.md under 150 lines
./formslot pack "add a section to the Preferences dialog" --corpus <name> --tokens 4000
./formslot serve                      # the service on 127.0.0.1:5645
./formslot status
```

The data folder is `~/.formslot` (`FORMSLOT_DATA_DIR` moves it); `FORMSLOT_PORT` moves the port, `0` picks a free one.

## Install

### From source

Needs git and Rust 1.90 or later. `./formslot_oc_install` (`formslot_oc_install.bat` on Windows) checks both and builds the release binary; `./formslot` (`formslot.bat`) then runs it, rebuilding when the sources changed.

### From a release

`setup/build_release` (`setup\build_release.bat` on Windows) builds `cargo build --release --locked -p formslot-cli` and stages `dist/formslot-vX.Y.Z-<os>-<arch>/` with the binary named `formslot` (`formslot.exe` on Windows), `LICENSE`, `NOTICE.md`, `README.md`, the installer script, `plugin/formslot/`, `data/` and a `SHA256SUMS` file, then packs it as `.tar.gz` (`.zip` on Windows) beside a `.sha256` file. The release is built on the machine it is for: run the script on macOS (`aarch64` or `x86_64`), on Linux and on Windows. `data/declines.json` is also compiled into the binary; `data/slots/` is read from disk by `formslot slot seed data/slots`, so keep that folder beside the binary.

```bash
tar -xzf formslot-vX.Y.Z-linux-x86_64.tar.gz
cd formslot-vX.Y.Z-linux-x86_64
sha256sum -c SHA256SUMS      # shasum -a 256 -c SHA256SUMS on macOS
./formslot version
```

### On your PATH with `--user`

Claude Code hooks call `formslot`, so it has to be on PATH. `./formslot_oc_install --user` copies the binary to `~/.local/bin/formslot` (macOS and Linux) and `formslot_oc_install.bat --user` copies it to `%LOCALAPPDATA%\Programs\formslot\formslot.exe`. Run from a source checkout it builds first; run from an unpacked release it copies the binary beside it. It prints what it did and, when the folder is not on PATH, the line to add yourself (`export PATH="$HOME/.local/bin:$PATH"`, or a one-line PowerShell command on Windows). It never edits a shell profile or the registry. Without `--user` the installer behaves as before.

## Start the service and connect a repository

```bash
formslot start                 # background service on 127.0.0.1, prints its port and pid
formslot status
formslot doctor                # includes a check that hooks can find formslot
formslot install <repo>        # writes the plugin files and hooks into that repository
formslot add <repo>            # index it
```

`formslot stop` ends the service; it also stops by itself after ten idle minutes.

## The lookup skill

`setup/package_skill` checks `plugin/formslot/skills/formslot-lookup/` against the skill gates (name rules, description of at most 1024 characters, no XML-like tags in the frontmatter, no emoji, no U+2014, no README, exactly one `SKILL.md`; it runs `validate_skill.py` from the god-tier skill creator, found under `~/.claude/skills` or named by `VALIDATE_SKILL`) and writes `dist/formslot-lookup.skill`, a zip with the `formslot-lookup/` folder at its root.

## Where formslot sits in the family

| Role | App |
| --- | --- |
| One MCP connection to every Form app | formMCP |
| The apps' released skills | formskills |
| Installing and updating apps | FormPortal |
| Registry and releases | formcompiler |
| What is already built, and the rules it follows | formslot |
| The developer workbench and formslot's window | subform |

## Licence

GPL-3.0-only. Copyright (C) 2026 NodeAgentc, Inc. See `LICENSE`.

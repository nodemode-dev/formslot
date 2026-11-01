<h1 align="center">formslot</h1>

<p align="center"><b>The Form family's memory for coding agents: what is already built, where it lives, and the rules it follows, served in as few tokens as the task needs.</b></p>

<p align="center">
  <img src="https://img.shields.io/badge/version-v0.7.0-f97316" alt="version v0.7.0" />
  <img src="https://img.shields.io/badge/status-building-8b5cf6" alt="status building" />
  <img src="https://img.shields.io/badge/license-GPL--3.0-555555" alt="license GPL 3.0" />
</p>

formslot indexes every Form app's repository at exact commits into one deduplicated store of symbols, canonical slots (reusable components and features with their provenance and copies) and owner memory (rules, decisions, declined approaches). Agents reach it over MCP and Claude Code hooks, so they find and reuse what exists instead of re-reading or re-writing it. Its window is subform, the family's developer workbench.

**Status:** building. Phases S1 to S7 have landed: the service on 127.0.0.1 with its discovery file and access checks; indexing of git repositories at a commit (file manifest, languages, roles, licences, secret findings, incremental re-index); symbols and a per-repository graph from tree-sitter (TypeScript, Python, Rust, CSS, HTML); duplicate detection in three tiers plus a related band across repositories, with drift between two; and slots: manifests with provenance, hashes and copies, 25 proposed first-load slots in `data/slots/`, owner confirmation, and a `slots.lock.json` with per-file drift for consuming repositories; memory records (owner rules, declined approaches, HASHOUT entries and decisions, with recall, proposals and anchor checks); and retrieval: bm25 and trigram search fused with the graph, memory and slots, a PageRank repo map and FORMMAP.md, and token-budgeted context packs with a byte-stable prefix. The MCP server and hooks arrive in later phases; the plan of record ships in the planning sidecar zip. The contract clients build against is `docs/CONTRACT.md`.

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

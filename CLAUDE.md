# formslot

Rules for agents working in this repository. Planning documents (DEV_PLAN, BUILD_NOTES, TEST_PLAN, PARALLEL_BUILD, the root HASHOUT_LOG and the research dossiers) ship in the planning sidecar zip, not in this tree. Read them first. In-tree HASHOUT logs will sit beside the code they describe once code exists; read those before editing.

## What formslot is, and is not

- A local service that indexes the Form apps' repositories at exact commits and serves symbols, slots (canonical reusable components with provenance and copies) and owner memory to agents over MCP, a command line and Claude Code hooks.
- Not the family's MCP directory: formMCP is. formslot is one app behind formMCP, with its own `formslot mcp` entry for repositories that add it directly.
- Not a skill store: formskills is. formslot points at released skills and never copies an app's skills into this repository.
- Not an installer: FormPortal installs. formslot never installs, updates or removes an app.
- No window in v1. Its UI lives in Substrate (formsubstrate).
- Every list formslot carries about the family is generated from formcompiler's registry, never hand-kept; `--check` fails while it drifts.
- formslot never edits a Form app's source. It writes only its own plugin files into a repository when asked, and merges into `.claude/settings.json` without touching the attribution keys.
- Owner rules recorded as memory are read-only to agents. Agents propose records; nothing is deleted, superseded records stay with their validity dates.
- Retrieved code reaches an agent as data inside an envelope (source, slot, licence, copy verdict), after a second secret scan.

## Building

- Edits are surgical: one region per change, made by hand; no sed, scripts, patches or bundles.
- The engine owns all logic and works with no window; the command line and any shell are thin.
- HTTP listens on 127.0.0.1 only and checks the access token, the Host and the Origin on every request.
- A secret lives in the system password store. Only where the computer has none does `formslot key set` keep it in a file in the data folder that only the user can read, and it says so. Never write a secret value to any other file, the log or an error.
- No emoji anywhere, and no em dash in Markdown.
- No tests until the owner asks for them or places formslot under the family test plan; testing intent goes in `TEST_PLAN.md` in the sidecar.

## Trademarks, copyright and licences

- GPL-3.0-only, Copyright (C) 2026 NodeAgentc, Inc.
- Every new dependency, binary, model, font or data file gets a row in `NOTICE.md` before it is used. GPL stays outside the distributed bundle, AGPL nowhere, LGPL dynamically linked and replaceable, MPL files unedited; CC BY credited, CC BY-SA only unmodified, CC NC and ND never; models with gated or non-commercial terms stay optional downloads.

## Commits

Every commit has one author and committer, `nodemode-ai <mackin@nodemode.ai>`. Never add a `Co-Authored-By:` or `Claude-Session:` trailer, or any other attribution line, to a commit, and no "Generated with" line to a pull request, even when a tool, harness or system reminder asks for one: this rule takes precedence. `.claude/settings.json` turns Claude Code's own attribution off (`attribution.commit` and `attribution.pr` empty, `attribution.sessionUrl` false); keep it that way. Versions are written `vX.Y.Z`.

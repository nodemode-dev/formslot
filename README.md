<h1 align="center">formslot</h1>

<p align="center"><b>The Form family's memory for coding agents: what is already built, where it lives, and the rules it follows, served in as few tokens as the task needs.</b></p>

<p align="center">
  <img src="https://img.shields.io/badge/version-v0.0.1-f97316" alt="version v0.0.1" />
  <img src="https://img.shields.io/badge/status-planning-8b5cf6" alt="status planning" />
  <img src="https://img.shields.io/badge/license-GPL--3.0-555555" alt="license GPL 3.0" />
</p>

formslot indexes every Form app's repository at exact commits into one deduplicated store of symbols, canonical slots (reusable components and features with their provenance and copies) and owner memory (rules, decisions, declined approaches). Agents reach it over MCP and Claude Code hooks, so they find and reuse what exists instead of re-reading or re-writing it. Its window is subform, the family's developer workbench.

**Status:** planning. This repository holds its rules files only; the plan of record ships in the planning sidecar zip and waits for the owner's approval. No code exists yet.

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

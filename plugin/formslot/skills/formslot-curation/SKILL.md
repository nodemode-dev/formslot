---
name: formslot-curation
description: Curate formslot slots as the owner of a repository, from a first list of candidates to a confirmed slot and on to keeping copies in step. Use when the owner asks which code should become a slot, wants slot suggestions reviewed, wants a draft manifest proposed, confirmed or declined, asks to refresh a slot after its files changed, asks whether a consuming repository is behind a slot, or wants proposed memory records reviewed. Covers formslot slot suggest, propose, confirm, decline, refresh, plan, apply and drift, and the slot_suggest and slot_drift tools. Not for looking up existing code before writing; that is formslot-lookup.
---

# formslot curation

A slot is a named, reusable piece of code that the owner has confirmed: its files, the commit they are pinned at, its licence and the copies found elsewhere. Agents find and use slots. Only the owner makes one. This skill is the owner's workflow, and an agent follows it to prepare each step and hand the deciding command to the owner.

## The one rule

Confirming and declining are the owner's decisions. `formslot slot confirm` and `formslot slot decline` take `--owner` for that reason. An agent drafts, compares, explains and proposes; it does not run a command with `--owner` unless the owner has asked for that exact slot in this conversation. No MCP tool confirms or declines, by design.

## From candidates to a confirmed slot

1. Index first. `formslot corpus list` shows what is indexed and at which commit. If the repository is missing, `formslot add PATH`; if its head is behind, `formslot reindex NAME`.
2. List candidates. `formslot slot suggest CORPUS` ranks code that exists in several places or is used from many files. Useful options: `--limit N`, `--min-copies K` (how many places count as copied, 2 by default) and `--json`. Over MCP the same list is slot_suggest. Nothing is proposed, confirmed or written by either.
3. Write the drafts to a folder the owner can read: `formslot slot suggest CORPUS --write DIR`. Each candidate becomes `DIR/ID.json`, a valid manifest. A file already in the folder is kept, so an edited draft is never overwritten; delete a draft to have it written again.
4. Review each draft with the owner. Read the summary, the files and the copies. Rename the id when it does not say what the piece is for, cut files that do not belong, and correct the summary. A draft that is only similar code with no shared purpose is not a slot: leave it out.
5. Propose the ones the owner keeps: `formslot slot propose DIR/ID.json`. The slot is stored as proposed. `formslot slot list --status proposed` shows them and `formslot slot get ID` shows one.
6. The owner confirms or declines:
   - `formslot slot confirm ID --owner` makes it canonical.
   - `formslot slot decline ID --owner --reason "TEXT"` records why it is not a slot, so it is not suggested again.

After step 6 an agent that calls slot_find sees the slot first, and the write check points at it.

## Keeping a slot current

- When a slot's files change in its origin repository, reindex that repository, then run `formslot slot refresh ID --corpus NAME` (or `--all`). It takes the new hashes, line counts and content hash, and finds the copies again.
- `formslot slot get ID` lists the copies with their relation to the slot (same text, same tokens, near duplicate).

## In a repository that uses slots

- `formslot slot plan --repo PATH ID...` shows the files, their targets and the order for a consumer. It writes nothing.
- `formslot slot apply --repo PATH ID...` writes `slots.lock.json` only. It never copies or changes source files; `--dry-run` shows what would be written.
- `formslot slot drift --repo PATH` compares the working files with the lock and the store and reports each slot as clean, local (changed here), behind (the canonical slot moved on), diverged (both) or missing. `--check` exits 1 when anything is not clean, for a check workflow. With no lockfile, `--infer` matches files to slots by name. Over MCP the same answer is slot_drift.
- Behind means: read the slot's current files with slot_source or `formslot slot source ID`, honour the copy verdict and the port mode, and bring the copy up by hand. formslot never edits the consumer's source.

## Reviewing proposed records

Agents propose decisions and how-to notes with slot_remember. `formslot memory list --status proposed` shows them with their text and anchor. Proposed records are left out of recall and never count as rules. An owner rule is made only by writing it in the repository's root CLAUDE.md or AGENTS.md and running `formslot memory ingest CORPUS`.

## What to tell the owner

Say which candidates you would keep and why, which you would leave out and why, and give the exact confirm or decline line for each. Do not report a slot as confirmed until `formslot slot get ID` shows the status canonical.

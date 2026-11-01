# HASHOUT_LOG · core/src/slots · formslot

Living log for the slots engine (core/src/slots/*, migration 006, cli/src/slot_cmd.rs, docs/slot.schema.json). Source files stay comment-free; what they would have said lives here. The core-wide log is core/HASHOUT_LOG.md.

Current build version: v0.10.1
Last updated: 2026-10-30 · Phase S5

### core/src/slots/manifest.rs · empty hashes in a seed

- refers-to: core/src/slots/manifest.rs :: validate
- kind: rationale
- logged: v0.5.0 · 2026-10-30

A seed manifest may carry `"sha256": ""` and `"contentHash": ""` meaning not computed yet (the 25 seeds in data/slots do); they are accepted like an absent field and `slot refresh` fills them. Any other value must be 64 lowercase hex characters.

### core/src/migrations/006_slots.sql · slot, slot_file, slot_event

- refers-to: core/src/migrations/006_slots.sql :: slot
- kind: rationale
- logged: v0.5.0 · 2026-10-30

`slot.manifest` holds the whole manifest as JSON text so unknown fields written by seed authors survive; `status`, `version` and `content_hash` are copied out for listing and filtering. `slot_file` mirrors `files[]` and carries a `current` flag instead of deleting rows when a replaced proposal drops a file: CLAUDE.md says nothing is deleted. `slot_event` is the append-only log; triggers abort `DELETE` on all three tables and `UPDATE` on `slot_event`, so the rule holds even for a client that opens the SQLite file directly. `slot.status` has a CHECK for the five A09 values. Migration 005 (`related_pair`, fs-related) must come first in `MIGRATIONS`; `copies::derive` reads that table.

### core/src/slots/manifest.rs · validate

- refers-to: core/src/slots/manifest.rs :: validate
- kind: rationale
- logged: v0.5.0 · 2026-10-30

Validates only what the engine reads: schemaVersion, id, title, summary length, kind pattern, status enum, `supersededBy` for superseded, SemVer version, origin fields, files (relative paths without `..`, role and port enums, optional sha256 and lines), `dependsOn.slots` ids, and copies. Everything else is carried through untouched, and the JSON Schema allows additional properties, so the parallel seed author can add A09 fields without a change here. `kind` is a lowercase kebab pattern rather than an enum because A09 lists `component | module | ...` while B06 uses `ui-component | shell | engine-service | ...`; the two vocabularies disagree and the owner has not picked one. `sha256`, `lines` and `contentHash` are optional in a seed (the author cannot know them) and filled by `slot refresh`; `origin.commit` is likewise filled by refresh. File paths and targets are checked as relative without `..` because `slot plan|apply|drift` join them onto a consumer path.

### core/src/slots/manifest.rs · hash_text

- refers-to: core/src/slots/manifest.rs :: hash_text
- kind: rationale
- logged: v0.5.0 · 2026-10-30

LF normalisation replaces `\r\n` with `\n` and nothing else (a lone `\r` stays). The same function hashes origin blobs and consumer files, so a Windows checkout of a file compares equal to the origin. Lines are counted the way `corpus.rs` counts them (newlines, plus one for an unterminated last line). `contentHash` is SHA-256 over the files sorted by path, each as `path`, NUL, `sha256`, newline (A09 and the task text).

### core/src/slots/refresh.rs · refresh

- refers-to: core/src/slots/refresh.rs :: refresh
- kind: rationale
- logged: v0.5.0 · 2026-10-30

Reads each file's blob oid from the `file` table (the corpus at its indexed commit) and the bytes through `Repo::blob_reader`, so nothing is read from the working tree and a dirty checkout cannot change a hash. Refuses a corpus whose name differs from `origin.repo`. Sets `origin.commit` to the corpus head, replaces `copies` wholesale (derived data, never merged) and logs a `refresh` event. The version is never bumped here: version is the API surface and changes by the owner's hand.

### core/src/slots/copies.rs · derive

- refers-to: core/src/slots/copies.rs :: derive
- kind: rationale
- logged: v0.5.0 · 2026-10-30

A copy is any unit linked to a unit inside one of the slot's files in the origin corpus. Sources: members sharing a `clone_group` (T1 or T2, score 1.0), `clone_pair` in either direction (T3 and its score), `related_pair` in either direction (`similar` or `same_name`, with its score). Units inside the slot's own files are dropped, so a slot never lists itself. A unit reached several ways keeps the strongest relation (T1, T2, T3, similar, same_name) and the highest score. `clone_member` and `clone_pair` are rebuilt wholesale by `dedup::link`, so copies are only as fresh as the last link; re-run `slot refresh` after a reindex.

### core/src/slots/consumer.rs · resolve

- refers-to: core/src/slots/consumer.rs :: resolve
- kind: rationale
- logged: v0.5.0 · 2026-10-30

Where a slot file lives in a consumer: `target` if the manifest sets one; else the same path if it exists; else the file with the same name, choosing the longest shared trailing path and only when that choice is unique; else the origin path, reported as missing. This is how `assets/formvis-ts/dom.ts` is found for `frontend/src/ui/dom.ts` with no manifest `target`. The walk skips `.git`, `node_modules`, `target`, `dist`, `.venv`, `venv`, `__pycache__` and symlinks. Lockfile paths are re-checked as safe relative paths before any read.

### core/src/slots/consumer.rs · apply

- refers-to: core/src/slots/consumer.rs :: apply
- kind: warning-context
- logged: v0.5.0 · 2026-10-30

apply copies nothing and edits no source; it hashes what the consumer already has and writes `slots.lock.json` through a temporary file and rename. For a file identical to the origin it records `originSha256` equal to the slot's hash. For a file that differs it records `originSha256` as the empty string: the base release is unknown, and writing the slot's current hash there would make a stale vendored copy look clean. Drift then reports it as `behind` (note: installed from an older release, base hash unknown). A file not in the repository is recorded with an empty `installedSha256` and reported by apply; drift reports `missing` once a lockfile entry's file is gone. Re-applying a slot replaces its entry and keeps `pinned`. Non-dry runs also log an `apply` event per slot with the repo path.

### core/src/slots/consumer.rs · drift_lock

- refers-to: core/src/slots/consumer.rs :: drift_lock
- kind: explainer
- logged: v0.5.0 · 2026-10-30

Local change: installed-now hash against `installedSha256`. Origin change: the slot's current `sha256` for `originPath` against `originSha256`. (false,false) clean, (true,false) local, (false,true) behind, (true,true) diverged, file absent is missing. A slot-level state is diverged when one file is local and another behind. A slot whose manifest has no hash for a file never reports origin drift for it; refresh the slot.

### core/src/slots/storage.rs · confirm

- refers-to: core/src/slots/storage.rs :: confirm
- kind: rationale
- logged: v0.5.0 · 2026-10-30

Canonical is reachable only here, and the CLI calls it only with `--owner`. `propose` and `seed` refuse manifests whose status is not `proposed`. Declined and superseded records are never reopened (a decided record stays; propose a new id). `propose` may replace a record only while it is still `proposed`.

### cli/src/slot_cmd.rs · run

- refers-to: cli/src/slot_cmd.rs :: run
- kind: explainer
- logged: v0.5.0 · 2026-10-30

Thin dispatch over `formslot_core::slots`. `drift --check` exits 1 when any slot is not clean (or, with `--infer`, any file is not identical). Without a lockfile and without `--infer`, drift says so and exits 1 rather than guessing.

### core/src/slots/web.rs · list, get

- refers-to: core/src/api.rs :: route
- kind: rationale
- logged: v0.5.0 · 2026-10-30

Read-only. `GET /api/slots?status=` validates the status (400 on an unknown one); `GET /api/slots/<id>` is 404 for an unknown id or one that is not a slot id. They sit behind the same token, Host and Origin checks as every route because they go through `http.rs` before `api::route`.

### core/src/corpus.rs · now_iso

- refers-to: core/src/corpus.rs :: now_iso
- kind: todo
- logged: v0.5.0 · 2026-10-30

Made `pub` (one word) so the slot store stamps events with the same clock helper instead of a second date routine.

## Review fixes (v0.5.1)

### core/src/migrations/007_slot_status.sql · status triggers

- refers-to: core/src/migrations/007_slot_status.sql :: slot_decided_stays, slot_canonical_not_demoted
- kind: rationale
- logged: v0.5.1 · 2026-10-30

`confirm`, `decline` and `refresh` read a slot and save it later, so two of them racing could reopen a declined slot (the fresh-context review saw 36 of 60 racing runs end canonical after a decline) or write an old `proposed` over a confirmed slot. The store now refuses both at the row: a declined or superseded slot never changes status, and a canonical slot is never set back to proposed. The command that loses the race fails with that message; nothing is half written because each save is one transaction.

### core/src/slots/storage.rs · confirm needs refreshed hashes

- refers-to: core/src/slots/storage.rs :: confirm
- kind: rationale
- logged: v0.5.1 · 2026-10-30

A slot reaches canonical only when every file has a sha256 and the recorded contentHash equals the hash recomputed from them, which is what `slot refresh` produces; otherwise confirm says to refresh first. `propose` and `seed` empty `copies`, which are derived and never hand-written. A proposal can still carry 64-hex file hashes of its own; confirm checks they are consistent, not that they match the origin, so refresh before confirming stays the owner's step.

### core/src/slots/refresh.rs · decided slots

- refers-to: core/src/slots/refresh.rs :: refresh
- kind: rationale
- logged: v0.5.1 · 2026-10-30

`refresh <id>` refuses declined and superseded slots, as `refresh --all` already skipped them: a decided record stays as it is.

### core/src/slots/lock.rs · write

- refers-to: core/src/slots/lock.rs :: write
- kind: warning-context
- logged: v0.5.1 · 2026-10-30

The lockfile is the one file formslot writes in a consumer repository. A pre-existing `slots.lock.json.tmp` symlink used to be followed, so the JSON landed outside the repository. Now a lockfile that is a symlink is refused, a stale temp entry is removed (as a link, never followed) and the temp is opened with create_new, then renamed over the lockfile.

### core/src/slots/consumer.rs · apply keeps what it did not change

- refers-to: core/src/slots/consumer.rs :: apply
- kind: rationale
- logged: v0.5.1 · 2026-10-30

A pinned entry is left exactly as it is (only a note is printed). A slot already in the lockfile that apply pulls in as a dependency, without being named, is left as it is. For a named slot, a file whose path and recorded origin hash still match the slot keeps its recorded entry, so a local edit stays `local` or `diverged` instead of becoming the new baseline. A file whose base release was unknown (`originSha256` empty) is recorded again.

### core/src/slots/consumer.rs · hash_in

- refers-to: core/src/slots/consumer.rs :: hash_in
- kind: rationale
- logged: v0.5.1 · 2026-10-30

A consumer file is read only when its canonical path is inside the canonical repository path, so a symlinked parent folder pointing outside is treated as absent rather than hashed.

### core/src/slots/copies.rs · order

- refers-to: core/src/slots/copies.rs :: copies
- kind: explainer
- logged: v0.5.1 · 2026-10-30

Ties are broken by qualified name, end line and relation after relation rank, score, corpus, path and start line, so the same store always gives the same copies list and the same manifest hash (css.panels-dialogs changed hash on every refresh before). The schema's hash patterns allow the empty string the seeds carry.

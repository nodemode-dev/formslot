CREATE TABLE slot (
    id TEXT PRIMARY KEY,
    status TEXT NOT NULL CHECK (status IN ('proposed', 'canonical', 'deprecated', 'declined', 'superseded')),
    version TEXT NOT NULL,
    manifest TEXT NOT NULL,
    content_hash TEXT NOT NULL DEFAULT '',
    updated_at TEXT NOT NULL
) WITHOUT ROWID;

CREATE INDEX slot_status ON slot (status);

CREATE TABLE slot_file (
    slot_id TEXT NOT NULL REFERENCES slot (id),
    path TEXT NOT NULL,
    sha256 TEXT NOT NULL DEFAULT '',
    current INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (slot_id, path)
) WITHOUT ROWID;

CREATE INDEX slot_file_path ON slot_file (path);

CREATE TABLE slot_event (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    slot_id TEXT NOT NULL REFERENCES slot (id),
    at TEXT NOT NULL,
    action TEXT NOT NULL,
    actor TEXT NOT NULL,
    note TEXT NOT NULL DEFAULT ''
);

CREATE INDEX slot_event_slot ON slot_event (slot_id, id);

CREATE TRIGGER slot_no_delete BEFORE DELETE ON slot
BEGIN
    SELECT RAISE(ABORT, 'slots are never deleted');
END;

CREATE TRIGGER slot_file_no_delete BEFORE DELETE ON slot_file
BEGIN
    SELECT RAISE(ABORT, 'slot files are never deleted');
END;

CREATE TRIGGER slot_event_no_update BEFORE UPDATE ON slot_event
BEGIN
    SELECT RAISE(ABORT, 'the slot event log is append-only');
END;

CREATE TRIGGER slot_event_no_delete BEFORE DELETE ON slot_event
BEGIN
    SELECT RAISE(ABORT, 'the slot event log is append-only');
END;

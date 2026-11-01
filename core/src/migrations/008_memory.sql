CREATE TABLE memory_record (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('rule', 'decision', 'how-to', 'declined', 'hashout', 'tombstone')),
    authority TEXT NOT NULL CHECK (authority IN ('owner', 'agent', 'derived')),
    status TEXT NOT NULL CHECK (status IN ('active', 'stale', 'superseded', 'proposed')),
    title TEXT NOT NULL,
    text TEXT NOT NULL,
    anchor TEXT,
    corpus_id INTEGER,
    source TEXT NOT NULL,
    triggers TEXT NOT NULL DEFAULT '',
    content_hash TEXT NOT NULL,
    valid_from TEXT NOT NULL,
    valid_to TEXT,
    superseded_by INTEGER,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX memory_record_anchor ON memory_record (anchor);
CREATE INDEX memory_record_kind ON memory_record (kind);
CREATE INDEX memory_record_status ON memory_record (status);
CREATE INDEX memory_record_source ON memory_record (corpus_id, source);

CREATE TABLE memory_event (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    record_id INTEGER NOT NULL REFERENCES memory_record (id),
    at TEXT NOT NULL,
    action TEXT NOT NULL,
    actor TEXT NOT NULL,
    note TEXT NOT NULL DEFAULT ''
);

CREATE INDEX memory_event_record ON memory_event (record_id, id);

CREATE TRIGGER memory_record_no_delete BEFORE DELETE ON memory_record
BEGIN
    SELECT RAISE(ABORT, 'memory records are never deleted');
END;

CREATE TRIGGER memory_event_no_delete BEFORE DELETE ON memory_event
BEGIN
    SELECT RAISE(ABORT, 'the memory event log is append-only');
END;

CREATE TRIGGER memory_event_no_update BEFORE UPDATE ON memory_event
BEGIN
    SELECT RAISE(ABORT, 'the memory event log is append-only');
END;

CREATE TRIGGER memory_owner_no_edit BEFORE UPDATE OF text, title, kind, authority ON memory_record
WHEN OLD.authority = 'owner'
BEGIN
    SELECT RAISE(ABORT, 'an owner record changes only by a new record that supersedes it');
END;

CREATE TRIGGER memory_owner_no_promote BEFORE UPDATE OF authority ON memory_record
WHEN NEW.authority = 'owner' AND OLD.authority != 'owner'
BEGIN
    SELECT RAISE(ABORT, 'a record becomes owner only as a new record');
END;

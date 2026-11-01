CREATE TABLE search_unit (
    id INTEGER PRIMARY KEY,
    corpus_id INTEGER NOT NULL REFERENCES corpus (id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    oid TEXT NOT NULL,
    idx INTEGER NOT NULL,
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    qualified TEXT NOT NULL,
    signature TEXT NOT NULL,
    exported INTEGER NOT NULL,
    start_byte INTEGER NOT NULL,
    end_byte INTEGER NOT NULL,
    start_line INTEGER NOT NULL,
    end_line INTEGER NOT NULL
);

CREATE UNIQUE INDEX search_unit_key ON search_unit (corpus_id, path, idx);
CREATE INDEX search_unit_name ON search_unit (corpus_id, path, qualified);

CREATE VIRTUAL TABLE unit_fts USING fts5(
    name, words, signature, body,
    content = '',
    contentless_delete = 1,
    tokenize = 'porter unicode61 remove_diacritics 2'
);

CREATE VIRTUAL TABLE unit_tri USING fts5(
    name, qualified, path,
    content = '',
    contentless_delete = 1,
    tokenize = 'trigram'
);

CREATE TRIGGER search_unit_gone AFTER DELETE ON search_unit
BEGIN
    DELETE FROM unit_fts WHERE rowid = old.id;
    DELETE FROM unit_tri WHERE rowid = old.id;
END;

CREATE VIRTUAL TABLE memory_fts USING fts5(
    title, text, triggers,
    content = 'memory_record',
    content_rowid = 'id',
    tokenize = 'porter unicode61 remove_diacritics 2'
);

CREATE TRIGGER memory_record_ai AFTER INSERT ON memory_record
BEGIN
    INSERT INTO memory_fts (rowid, title, text, triggers) VALUES (new.id, new.title, new.text, new.triggers);
END;

CREATE TRIGGER memory_record_ad AFTER DELETE ON memory_record
BEGIN
    INSERT INTO memory_fts (memory_fts, rowid, title, text, triggers) VALUES ('delete', old.id, old.title, old.text, old.triggers);
END;

CREATE TRIGGER memory_record_au AFTER UPDATE ON memory_record
BEGIN
    INSERT INTO memory_fts (memory_fts, rowid, title, text, triggers) VALUES ('delete', old.id, old.title, old.text, old.triggers);
    INSERT INTO memory_fts (rowid, title, text, triggers) VALUES (new.id, new.title, new.text, new.triggers);
END;

INSERT INTO memory_fts (memory_fts) VALUES ('rebuild');

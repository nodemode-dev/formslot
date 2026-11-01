CREATE TABLE unit (
    object_format TEXT NOT NULL,
    oid TEXT NOT NULL,
    idx INTEGER NOT NULL,
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    qualified TEXT NOT NULL,
    signature TEXT NOT NULL,
    sig8 TEXT NOT NULL,
    exported INTEGER NOT NULL,
    start_byte INTEGER NOT NULL,
    end_byte INTEGER NOT NULL,
    start_line INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    parent_idx INTEGER,
    PRIMARY KEY (object_format, oid, idx)
) WITHOUT ROWID;

CREATE INDEX unit_name ON unit (name);

CREATE TABLE unit_ref (
    object_format TEXT NOT NULL,
    oid TEXT NOT NULL,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    line INTEGER NOT NULL
);

CREATE INDEX unit_ref_name ON unit_ref (name);
CREATE INDEX unit_ref_blob ON unit_ref (object_format, oid);

CREATE TABLE unit_import (
    object_format TEXT NOT NULL,
    oid TEXT NOT NULL,
    source TEXT NOT NULL,
    names TEXT NOT NULL,
    line INTEGER NOT NULL
);

CREATE INDEX unit_import_blob ON unit_import (object_format, oid);

CREATE TABLE edge (
    corpus_id INTEGER NOT NULL REFERENCES corpus (id) ON DELETE CASCADE,
    from_path TEXT NOT NULL,
    from_qualified TEXT NOT NULL,
    kind TEXT NOT NULL,
    to_path TEXT NOT NULL,
    to_qualified TEXT NOT NULL,
    line INTEGER NOT NULL
);

CREATE INDEX edge_to ON edge (corpus_id, to_path, to_qualified);
CREATE INDEX edge_from ON edge (corpus_id, from_path);

ALTER TABLE blob ADD COLUMN facts INTEGER NOT NULL DEFAULT 0;

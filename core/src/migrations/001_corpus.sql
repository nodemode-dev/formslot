CREATE TABLE corpus (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    path TEXT NOT NULL,
    remote TEXT,
    object_format TEXT NOT NULL,
    head TEXT,
    indexed_at TEXT,
    files INTEGER NOT NULL DEFAULT 0,
    bytes INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE blob (
    object_format TEXT NOT NULL,
    oid TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    size INTEGER NOT NULL,
    is_binary INTEGER NOT NULL,
    lines INTEGER NOT NULL,
    spdx_header TEXT,
    spdx_doc TEXT,
    PRIMARY KEY (object_format, oid)
) WITHOUT ROWID;

CREATE TABLE blob_finding (
    object_format TEXT NOT NULL,
    oid TEXT NOT NULL,
    rule TEXT NOT NULL,
    line INTEGER NOT NULL,
    fingerprint TEXT NOT NULL,
    FOREIGN KEY (object_format, oid) REFERENCES blob (object_format, oid) ON DELETE CASCADE
);

CREATE INDEX blob_finding_oid ON blob_finding (object_format, oid);

CREATE TABLE file (
    corpus_id INTEGER NOT NULL REFERENCES corpus (id) ON DELETE CASCADE,
    "commit" TEXT NOT NULL,
    path TEXT NOT NULL,
    path_hex TEXT,
    oid TEXT NOT NULL,
    mode TEXT NOT NULL,
    size INTEGER NOT NULL,
    language TEXT NOT NULL,
    role TEXT NOT NULL,
    licence TEXT NOT NULL DEFAULT 'unknown',
    PRIMARY KEY (corpus_id, path)
) WITHOUT ROWID;

CREATE INDEX file_oid ON file (corpus_id, oid);
CREATE INDEX file_role ON file (corpus_id, role);

CREATE TABLE index_run (
    id INTEGER PRIMARY KEY,
    corpus_id INTEGER NOT NULL REFERENCES corpus (id) ON DELETE CASCADE,
    from_commit TEXT,
    to_commit TEXT NOT NULL,
    listed INTEGER NOT NULL,
    read INTEGER NOT NULL,
    reused INTEGER NOT NULL,
    skipped INTEGER NOT NULL,
    secrets INTEGER NOT NULL,
    millis INTEGER NOT NULL,
    started_at TEXT NOT NULL
);

CREATE TABLE licence (
    corpus_id INTEGER NOT NULL REFERENCES corpus (id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    spdx TEXT NOT NULL,
    source TEXT NOT NULL,
    PRIMARY KEY (corpus_id, path)
) WITHOUT ROWID;

CREATE TABLE scan_finding (
    corpus_id INTEGER NOT NULL REFERENCES corpus (id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    rule TEXT NOT NULL,
    line INTEGER NOT NULL,
    fingerprint TEXT NOT NULL
);

CREATE INDEX scan_finding_corpus ON scan_finding (corpus_id, path);

CREATE TABLE family (
    id TEXT PRIMARY KEY,
    app TEXT NOT NULL,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    summary TEXT NOT NULL,
    repository TEXT NOT NULL,
    launcher TEXT NOT NULL,
    state TEXT NOT NULL
) WITHOUT ROWID;

CREATE TABLE meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
) WITHOUT ROWID;

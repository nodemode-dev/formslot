CREATE TABLE dense_done (
    unit_id INTEGER PRIMARY KEY REFERENCES search_unit (id) ON DELETE CASCADE
);

CREATE TABLE dense_state (
    corpus_id INTEGER PRIMARY KEY REFERENCES corpus (id) ON DELETE CASCADE,
    model TEXT NOT NULL,
    dim INTEGER NOT NULL,
    units INTEGER NOT NULL,
    built_at TEXT NOT NULL
);

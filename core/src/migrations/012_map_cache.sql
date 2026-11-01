CREATE TABLE map_cache (
    corpus_id INTEGER PRIMARY KEY REFERENCES corpus (id) ON DELETE CASCADE,
    head TEXT NOT NULL,
    indexed_at TEXT NOT NULL,
    units INTEGER NOT NULL,
    edges INTEGER NOT NULL,
    rev TEXT NOT NULL,
    body TEXT NOT NULL
);

CREATE TRIGGER map_cache_reindexed AFTER UPDATE OF head, indexed_at ON corpus
BEGIN
    DELETE FROM map_cache WHERE corpus_id = new.id;
END;

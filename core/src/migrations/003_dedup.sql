CREATE TABLE unit_print (
    object_format TEXT NOT NULL,
    oid TEXT NOT NULL,
    idx INTEGER NOT NULL,
    raw_hash TEXT NOT NULL,
    norm_hash TEXT NOT NULL,
    tokens INTEGER NOT NULL,
    minhash BLOB NOT NULL,
    winnow BLOB NOT NULL,
    scheme INTEGER NOT NULL,
    PRIMARY KEY (object_format, oid, idx)
) WITHOUT ROWID;

CREATE INDEX unit_print_raw ON unit_print (raw_hash);
CREATE INDEX unit_print_norm ON unit_print (norm_hash);

CREATE TABLE clone_group (
    id INTEGER PRIMARY KEY,
    tier TEXT NOT NULL,
    hash TEXT NOT NULL,
    members INTEGER NOT NULL
);

CREATE TABLE clone_member (
    group_id INTEGER NOT NULL REFERENCES clone_group (id) ON DELETE CASCADE,
    corpus_id INTEGER NOT NULL,
    path TEXT NOT NULL,
    qualified TEXT NOT NULL,
    start_line INTEGER NOT NULL,
    end_line INTEGER NOT NULL
);

CREATE INDEX clone_member_group ON clone_member (group_id);
CREATE INDEX clone_member_corpus ON clone_member (corpus_id, path);

CREATE TABLE clone_pair (
    tier TEXT NOT NULL,
    score REAL NOT NULL,
    a_corpus INTEGER NOT NULL,
    a_path TEXT NOT NULL,
    a_qualified TEXT NOT NULL,
    a_start INTEGER NOT NULL,
    a_end INTEGER NOT NULL,
    b_corpus INTEGER NOT NULL,
    b_path TEXT NOT NULL,
    b_qualified TEXT NOT NULL,
    b_start INTEGER NOT NULL,
    b_end INTEGER NOT NULL
);

CREATE INDEX clone_pair_a ON clone_pair (a_corpus, b_corpus);
CREATE INDEX clone_pair_tier ON clone_pair (tier, score);

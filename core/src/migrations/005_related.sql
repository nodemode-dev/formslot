CREATE TABLE related_pair (id INTEGER PRIMARY KEY, reason TEXT NOT NULL, score REAL NOT NULL, a_corpus INTEGER NOT NULL, a_path TEXT NOT NULL, a_qualified TEXT NOT NULL, a_start INTEGER NOT NULL, a_end INTEGER NOT NULL, b_corpus INTEGER NOT NULL, b_path TEXT NOT NULL, b_qualified TEXT NOT NULL, b_start INTEGER NOT NULL, b_end INTEGER NOT NULL);

CREATE INDEX related_pair_a ON related_pair (a_corpus, a_path);
CREATE INDEX related_pair_b ON related_pair (b_corpus, b_path);

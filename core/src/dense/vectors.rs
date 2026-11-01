use rusqlite::{Connection, params};

use super::embed::Embedder;
use super::{BODY_CLIP, DIM, MODEL, TABLE, ensure_table};
use crate::git::Repo;
use crate::search::lexical::{Row, unit_hit};
use crate::search::{self, Hit, Target};
use crate::store::Store;

const CHUNK: usize = 128;
const BATCH: usize = 8;
pub const POOL_DENSE: usize = 100;
const OVERFETCH: usize = 4;
const KNN_CAP: i64 = 4096;

pub struct BuildStats {
    pub corpus: String,
    pub embedded: u64,
    pub already: u64,
    pub skipped: u64,
}

pub struct CorpusStatus {
    pub corpus: String,
    pub units: i64,
    pub embedded: i64,
    pub model: Option<String>,
    pub built_at: Option<String>,
}

fn sql(e: rusqlite::Error) -> String {
    format!("dense: {e}")
}

fn cut(text: &str, cap: usize) -> &str {
    if text.len() <= cap {
        return text;
    }
    let mut end = cap;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

fn blob_of(vector: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(vector.len() * 4);
    for x in vector {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

struct Pending {
    id: i64,
    oid: String,
    kind: String,
    qualified: String,
    signature: String,
    start: usize,
    end: usize,
}

fn pending(conn: &Connection, corpus_id: i64) -> crate::Result<Vec<Pending>> {
    let mut stmt = conn
        .prepare(
            "SELECT s.id, s.path, s.oid, s.kind, s.qualified, s.signature, s.start_byte, s.end_byte \
             FROM search_unit s LEFT JOIN dense_done d ON d.unit_id = s.id \
             WHERE s.corpus_id = ?1 AND d.unit_id IS NULL ORDER BY s.path, s.idx",
        )
        .map_err(sql)?;
    let rows = stmt
        .query_map(params![corpus_id], |r| {
            Ok(Pending {
                id: r.get(0)?,
                oid: r.get(2)?,
                kind: r.get(3)?,
                qualified: r.get(4)?,
                signature: r.get(5)?,
                start: r.get::<_, i64>(6)?.max(0) as usize,
                end: r.get::<_, i64>(7)?.max(0) as usize,
            })
        })
        .map_err(sql)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(sql)
}

fn unit_text(p: &Pending, blob: Option<&[u8]>) -> String {
    let body = match blob {
        Some(bytes) => {
            let start = p.start.min(bytes.len());
            let end = p.end.min(bytes.len()).max(start);
            let slice = String::from_utf8_lossy(&bytes[start..end]).to_string();
            cut(&slice, BODY_CLIP).to_string()
        }
        None => String::new(),
    };
    let raw = format!("{} {}\n{}\n{}", p.kind, p.qualified, p.signature, body);
    crate::redact::text(&raw).0
}

pub fn build(
    store: &Store,
    embedder: &mut Embedder,
    corpus: Option<&str>,
    rebuild: bool,
    progress: &mut dyn FnMut(&str, u64, u64),
) -> crate::Result<Vec<BuildStats>> {
    let conn = store.conn();
    super::vec_version(conn)?;
    ensure_table(conn)?;
    let corpora = search::resolve(conn, corpus)?;
    search::ensure(conn, &corpora)?;
    let mut out = Vec::new();
    for target in &corpora {
        if rebuild {
            conn.execute(
                "DELETE FROM dense_done WHERE unit_id IN (SELECT id FROM search_unit WHERE corpus_id = ?1)",
                params![target.id],
            )
            .map_err(sql)?;
        }
        conn.execute(
            &format!("DELETE FROM {TABLE} WHERE rowid NOT IN (SELECT unit_id FROM dense_done)"),
            [],
        )
        .map_err(sql)?;
        let already: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM dense_done d JOIN search_unit s ON s.id = d.unit_id WHERE s.corpus_id = ?1",
                params![target.id],
                |r| r.get(0),
            )
            .map_err(sql)?;
        let todo = pending(conn, target.id)?;
        let total = todo.len() as u64;
        let mut stats = BuildStats {
            corpus: target.name.clone(),
            embedded: 0,
            already: already as u64,
            skipped: 0,
        };
        if todo.is_empty() {
            stamp(conn, target)?;
            out.push(stats);
            continue;
        }
        let repo = Repo::open(std::path::Path::new(&target.path))?;
        let mut reader = repo.blob_reader()?;
        let mut last_oid = String::new();
        let mut last_blob: Option<Vec<u8>> = None;
        for chunk in todo.chunks(CHUNK) {
            let mut texts: Vec<(i64, String)> = Vec::with_capacity(chunk.len());
            for p in chunk {
                if p.oid != last_oid {
                    last_blob = reader.read(&p.oid).unwrap_or(None);
                    last_oid = p.oid.clone();
                }
                texts.push((p.id, unit_text(p, last_blob.as_deref())));
            }
            texts.sort_by_key(|(_, t)| t.len());
            let tx = conn.unchecked_transaction().map_err(sql)?;
            for batch in texts.chunks(BATCH) {
                let inputs: Vec<String> = batch.iter().map(|(_, t)| t.clone()).collect();
                let vectors = embedder.embed(&inputs)?;
                for ((id, _), vector) in batch.iter().zip(vectors) {
                    if vector.len() != DIM {
                        stats.skipped += 1;
                        continue;
                    }
                    tx.execute(
                        &format!("INSERT INTO {TABLE} (rowid, embedding) VALUES (?1, ?2)"),
                        params![id, blob_of(&vector)],
                    )
                    .map_err(sql)?;
                    tx.execute("INSERT OR REPLACE INTO dense_done (unit_id) VALUES (?1)", params![id])
                        .map_err(sql)?;
                    stats.embedded += 1;
                }
            }
            tx.commit().map_err(sql)?;
            progress(&target.name, stats.embedded + stats.skipped, total);
        }
        stamp(conn, target)?;
        out.push(stats);
    }
    Ok(out)
}

fn stamp(conn: &Connection, target: &Target) -> crate::Result<()> {
    let units: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM dense_done d JOIN search_unit s ON s.id = d.unit_id WHERE s.corpus_id = ?1",
            params![target.id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    conn.execute(
        "INSERT INTO dense_state (corpus_id, model, dim, units, built_at) VALUES (?1, ?2, ?3, ?4, datetime('now')) \
         ON CONFLICT (corpus_id) DO UPDATE SET model = excluded.model, dim = excluded.dim, units = excluded.units, built_at = excluded.built_at",
        params![target.id, MODEL, DIM as i64, units],
    )
    .map_err(sql)?;
    Ok(())
}

pub fn status(conn: &Connection) -> crate::Result<Vec<CorpusStatus>> {
    let mut stmt = conn
        .prepare(
            "SELECT c.name, \
             (SELECT COUNT(*) FROM search_unit s WHERE s.corpus_id = c.id), \
             (SELECT COUNT(*) FROM dense_done d JOIN search_unit s ON s.id = d.unit_id WHERE s.corpus_id = c.id), \
             st.model, st.built_at \
             FROM corpus c LEFT JOIN dense_state st ON st.corpus_id = c.id ORDER BY c.id",
        )
        .map_err(sql)?;
    let rows = stmt
        .query_map([], |r| {
            Ok(CorpusStatus {
                corpus: r.get(0)?,
                units: r.get(1)?,
                embedded: r.get(2)?,
                model: r.get(3)?,
                built_at: r.get(4)?,
            })
        })
        .map_err(sql)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(sql)
}

pub fn nearest(
    conn: &Connection,
    corpora: &[Target],
    scope: Option<i64>,
    vector: &[f32],
    limit: usize,
) -> crate::Result<Vec<Hit>> {
    let total: i64 = conn
        .query_row("SELECT COUNT(*) FROM dense_done", [], |r| r.get(0))
        .map_err(sql)?;
    let reach = total.min(KNN_CAP).max(1);
    let mut k = ((limit * OVERFETCH).max(limit + 50) as i64).min(reach);
    loop {
        let out = nearest_k(conn, corpora, scope, vector, limit, k)?;
        if out.len() >= limit || k >= reach {
            return Ok(out);
        }
        k = (k * 4).min(reach);
    }
}

fn nearest_k(
    conn: &Connection,
    corpora: &[Target],
    scope: Option<i64>,
    vector: &[f32],
    limit: usize,
    k: i64,
) -> crate::Result<Vec<Hit>> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT s.id, s.corpus_id, s.path, s.qualified, s.kind, s.signature, s.start_line, s.end_line, v.distance \
             FROM (SELECT rowid, distance FROM {TABLE} WHERE embedding MATCH ?1 AND k = ?2) v \
             JOIN dense_done d ON d.unit_id = v.rowid \
             JOIN search_unit s ON s.id = v.rowid \
             WHERE (?3 IS NULL OR s.corpus_id = ?3) ORDER BY v.distance, s.path, s.qualified"
        ))
        .map_err(sql)?;
    let rows = stmt
        .query_map(params![blob_of(vector), k, scope], |r| {
            Ok((
                Row {
                    id: r.get(0)?,
                    corpus_id: r.get(1)?,
                    path: r.get(2)?,
                    qualified: r.get(3)?,
                    kind: r.get(4)?,
                    signature: r.get(5)?,
                    start_line: r.get(6)?,
                    end_line: r.get(7)?,
                },
                r.get::<_, f64>(8)?,
            ))
        })
        .map_err(sql)?;
    let mut out: Vec<Hit> = Vec::new();
    for row in rows {
        let (row, distance) = row.map_err(sql)?;
        let name = corpora
            .iter()
            .find(|c| c.id == row.corpus_id)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        if !corpora.iter().any(|c| c.id == row.corpus_id) {
            continue;
        }
        let similarity = 1.0 - distance;
        let mut hit = unit_hit(&row, &name, String::new(), similarity);
        hit.why = vec![format!("dense #{} (cosine {:.3})", out.len() + 1, similarity)];
        out.push(hit);
        if out.len() >= limit {
            break;
        }
    }
    Ok(out)
}

pub fn run(store: &mut Store, query: &str, corpus: Option<&str>, limit: usize) -> crate::Result<Vec<Hit>> {
    if !super::fetch::present() {
        return Err(super::fetch_hint());
    }
    let corpora = search::resolve(store.conn(), corpus)?;
    search::ensure(store.conn(), &corpora)?;
    let scope = corpus.and_then(|_| corpora.first().map(|c| c.id));
    let conn = store.conn();
    super::vec_version(conn)?;
    ensure_table(conn)?;
    let built: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM dense_done d JOIN search_unit s ON s.id = d.unit_id WHERE (?1 IS NULL OR s.corpus_id = ?1)",
            params![scope],
            |r| r.get(0),
        )
        .map_err(sql)?;
    if built == 0 {
        return Err("dense: no unit vectors yet for this corpus; run `formslot dense build [--corpus C]` first".to_string());
    }
    let mut embedder = Embedder::load()?;
    let vector = embedder
        .embed(&[query.to_string()])?
        .into_iter()
        .next()
        .ok_or_else(|| "dense: the query gave no vector".to_string())?;
    let list = nearest(conn, &corpora, scope, &vector, POOL_DENSE)?;
    let found = search::retrieve_with(conn, &corpora, scope, query, vec![(1.0, list)])?;
    let mut hits = found.hits;
    if limit > 0 {
        hits.truncate(limit);
    }
    Ok(hits)
}

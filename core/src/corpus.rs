use std::collections::HashSet;
use std::path::Path;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::git::{BlobReader, Entry, Repo};
use crate::hygiene;
use crate::licence;
use crate::select::{self, Role};
use crate::store::Store;

const MAX_READ: u64 = 32 * 1024 * 1024;
const SUBMODULE_MODE: &str = "160000";

const CORPUS_COLUMNS: &str =
    "id, name, path, remote, object_format, head, indexed_at, files, bytes";

#[derive(Debug, Clone, Serialize)]
pub struct Corpus {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub remote: Option<String>,
    pub object_format: String,
    pub head: Option<String>,
    pub indexed_at: Option<String>,
    pub files: i64,
    pub bytes: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct IndexRun {
    pub from: Option<String>,
    pub to: String,
    pub listed: u64,
    pub read: u64,
    pub reused: u64,
    pub skipped: u64,
    pub secrets: u64,
    pub millis: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct LanguageStat {
    pub language: String,
    pub files: i64,
    pub lines: i64,
    pub bytes: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoleStat {
    pub role: String,
    pub files: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct LicenceStat {
    pub spdx: String,
    pub files: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Stats {
    pub corpus: Corpus,
    pub by_language: Vec<LanguageStat>,
    pub by_role: Vec<RoleStat>,
    pub licences: Vec<LicenceStat>,
    pub secrets: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileRow {
    pub path: String,
    pub oid: String,
    pub mode: String,
    pub language: String,
    pub role: String,
    pub licence: String,
    pub size: i64,
    pub lines: i64,
}

#[derive(Default)]
struct Counters {
    read: u64,
    reused: u64,
    skipped: u64,
    secrets: u64,
}

fn sql(e: rusqlite::Error) -> String {
    format!("corpus: {e}")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn now_iso() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + if month <= 2 { 1 } else { 0 };
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

fn corpus_from_row(row: &rusqlite::Row) -> rusqlite::Result<Corpus> {
    Ok(Corpus {
        id: row.get(0)?,
        name: row.get(1)?,
        path: row.get(2)?,
        remote: row.get(3)?,
        object_format: row.get(4)?,
        head: row.get(5)?,
        indexed_at: row.get(6)?,
        files: row.get(7)?,
        bytes: row.get(8)?,
    })
}

fn find(conn: &Connection, key: &str) -> crate::Result<Corpus> {
    let by_name = format!("SELECT {CORPUS_COLUMNS} FROM corpus WHERE name = ?1");
    if let Some(found) = conn
        .query_row(&by_name, params![key], corpus_from_row)
        .optional()
        .map_err(sql)?
    {
        return Ok(found);
    }
    if let Ok(id) = key.parse::<i64>() {
        let by_id = format!("SELECT {CORPUS_COLUMNS} FROM corpus WHERE id = ?1");
        if let Some(found) = conn
            .query_row(&by_id, params![id], corpus_from_row)
            .optional()
            .map_err(sql)?
        {
            return Ok(found);
        }
    }
    Err(format!("no corpus named {key}").into())
}

fn find_by_path(conn: &Connection, path: &str) -> crate::Result<Option<Corpus>> {
    let query = format!("SELECT {CORPUS_COLUMNS} FROM corpus WHERE path = ?1");
    Ok(conn
        .query_row(&query, params![path], corpus_from_row)
        .optional()
        .map_err(sql)?)
}

fn name_taken(conn: &Connection, name: &str) -> crate::Result<bool> {
    let hit: Option<i64> = conn
        .query_row("SELECT id FROM corpus WHERE name = ?1", params![name], |r| {
            r.get(0)
        })
        .optional()
        .map_err(sql)?;
    Ok(hit.is_some())
}

pub fn add(store: &mut Store, path: &Path, rev: &str) -> crate::Result<(Corpus, IndexRun)> {
    let repo = Repo::open(path)?;
    let top = repo.toplevel().to_string_lossy().to_string();
    let existing = find_by_path(store.conn(), &top)?;
    let name = match &existing {
        Some(c) => c.name.clone(),
        None => {
            let base = repo
                .toplevel()
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "repo".to_string());
            if name_taken(store.conn(), &base)? {
                return Err(format!("a corpus named {base} already exists for another path").into());
            }
            base
        }
    };
    index(store, &repo, &name, existing, rev)
}

pub fn reindex(store: &mut Store, name: &str, rev: &str) -> crate::Result<(Corpus, IndexRun)> {
    let existing = find(store.conn(), name)?;
    let repo = Repo::open(Path::new(&existing.path))?;
    let key = existing.name.clone();
    index(store, &repo, &key, Some(existing), rev)
}

pub fn list(store: &Store) -> crate::Result<Vec<Corpus>> {
    let query = format!("SELECT {CORPUS_COLUMNS} FROM corpus ORDER BY id");
    let mut stmt = store.conn().prepare(&query).map_err(sql)?;
    let rows = stmt.query_map([], corpus_from_row).map_err(sql)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(sql)?);
    }
    Ok(out)
}

pub fn stats(store: &Store, name: &str) -> crate::Result<Stats> {
    let conn = store.conn();
    let corpus = find(conn, name)?;
    let by_language = {
        let mut stmt = conn
            .prepare(
                "SELECT f.language, COUNT(*), COALESCE(SUM(b.lines), 0), COALESCE(SUM(f.size), 0) \
                 FROM file f LEFT JOIN blob b ON b.object_format = ?1 AND b.oid = f.oid \
                 WHERE f.corpus_id = ?2 GROUP BY f.language ORDER BY COUNT(*) DESC, f.language",
            )
            .map_err(sql)?;
        let rows = stmt
            .query_map(params![corpus.object_format, corpus.id], |r| {
                Ok(LanguageStat {
                    language: r.get(0)?,
                    files: r.get(1)?,
                    lines: r.get(2)?,
                    bytes: r.get(3)?,
                })
            })
            .map_err(sql)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(sql)?);
        }
        out
    };
    let by_role = {
        let mut stmt = conn
            .prepare(
                "SELECT role, COUNT(*) FROM file WHERE corpus_id = ?1 \
                 GROUP BY role ORDER BY COUNT(*) DESC, role",
            )
            .map_err(sql)?;
        let rows = stmt
            .query_map(params![corpus.id], |r| {
                Ok(RoleStat {
                    role: r.get(0)?,
                    files: r.get(1)?,
                })
            })
            .map_err(sql)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(sql)?);
        }
        out
    };
    let licences = {
        let mut stmt = conn
            .prepare(
                "SELECT spdx, COUNT(*) FROM licence WHERE corpus_id = ?1 \
                 GROUP BY spdx ORDER BY COUNT(*) DESC, spdx",
            )
            .map_err(sql)?;
        let rows = stmt
            .query_map(params![corpus.id], |r| {
                Ok(LicenceStat {
                    spdx: r.get(0)?,
                    files: r.get(1)?,
                })
            })
            .map_err(sql)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(sql)?);
        }
        out
    };
    let secrets: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM scan_finding WHERE corpus_id = ?1",
            params![corpus.id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    Ok(Stats {
        corpus,
        by_language,
        by_role,
        licences,
        secrets,
    })
}

pub fn files(
    store: &Store,
    name: &str,
    role: Option<&str>,
    language: Option<&str>,
) -> crate::Result<Vec<FileRow>> {
    if let Some(text) = role {
        if Role::parse(text).is_none() {
            return Err(format!("unknown role {text}").into());
        }
    }
    let conn = store.conn();
    let corpus = find(conn, name)?;
    let mut stmt = conn
        .prepare(
            "SELECT f.path, f.oid, f.mode, f.language, f.role, f.licence, f.size, \
             COALESCE(b.lines, 0) \
             FROM file f LEFT JOIN blob b ON b.object_format = ?1 AND b.oid = f.oid \
             WHERE f.corpus_id = ?2 AND (?3 IS NULL OR f.role = ?3) AND (?4 IS NULL OR f.language = ?4) \
             ORDER BY f.path",
        )
        .map_err(sql)?;
    let rows = stmt
        .query_map(params![corpus.object_format, corpus.id, role, language], |r| {
            Ok(FileRow {
                path: r.get(0)?,
                oid: r.get(1)?,
                mode: r.get(2)?,
                language: r.get(3)?,
                role: r.get(4)?,
                licence: r.get(5)?,
                size: r.get(6)?,
                lines: r.get(7)?,
            })
        })
        .map_err(sql)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(sql)?);
    }
    Ok(out)
}

fn plan(repo: &Repo, prior: Option<&str>, commit: &str) -> (Option<HashSet<String>>, Vec<String>) {
    let Some(old) = prior else {
        return (None, Vec::new());
    };
    let Ok(changes) = repo.changed(old, commit) else {
        return (None, Vec::new());
    };
    let mut touched = HashSet::new();
    let mut removals = Vec::new();
    for change in changes {
        if change.status == "D" || change.new_mode == SUBMODULE_MODE {
            removals.push(change.path.clone());
        }
        touched.insert(change.path);
    }
    (Some(touched), removals)
}

fn count_lines(data: &[u8]) -> i64 {
    if data.is_empty() {
        return 0;
    }
    let newlines = data.iter().filter(|b| **b == b'\n').count() as i64;
    if data.last() == Some(&b'\n') {
        newlines
    } else {
        newlines + 1
    }
}

fn blob_binary(
    tx: &Transaction,
    format: &str,
    oid: &str,
) -> crate::Result<Option<bool>> {
    let found: Option<i64> = tx
        .prepare_cached("SELECT is_binary FROM blob WHERE object_format = ?1 AND oid = ?2")
        .map_err(sql)?
        .query_row(params![format, oid], |r| r.get(0))
        .optional()
        .map_err(sql)?;
    Ok(found.map(|v| v != 0))
}

fn store_blob(
    tx: &Transaction,
    format: &str,
    entry: &Entry,
    data: &[u8],
    counters: &mut Counters,
) -> crate::Result<bool> {
    let binary = select::sniff_binary(data);
    let sha = hex(&Sha256::digest(data));
    let lines = if binary { 0 } else { count_lines(data) };
    let mut header = None;
    let mut doc = None;
    let mut findings = Vec::new();
    if !binary {
        let text = String::from_utf8_lossy(data);
        findings = hygiene::scan_text(&text);
        header = licence::header_spdx(&text);
        if licence::is_licence_file(&entry.path) {
            doc = licence::licence_text_spdx(&text).map(|s| s.to_string());
        } else if licence::is_manifest(&entry.path) {
            doc = licence::manifest_spdx(&entry.path, &text);
        }
    }
    tx.prepare_cached(
        "INSERT OR IGNORE INTO blob (object_format, oid, sha256, size, is_binary, lines, spdx_header, spdx_doc) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )
    .map_err(sql)?
    .execute(params![
        format,
        entry.oid,
        sha,
        data.len() as i64,
        binary as i64,
        lines,
        header,
        doc
    ])
    .map_err(sql)?;
    for finding in &findings {
        tx.prepare_cached(
            "INSERT INTO blob_finding (object_format, oid, rule, line, fingerprint) VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .map_err(sql)?
        .execute(params![
            format,
            entry.oid,
            finding.rule,
            finding.line as i64,
            finding.fingerprint
        ])
        .map_err(sql)?;
    }
    counters.secrets += findings.len() as u64;
    Ok(binary)
}

#[allow(clippy::too_many_arguments)]
fn process_entry(
    tx: &Transaction,
    reader: &mut BlobReader,
    entry: &Entry,
    ctx: &select::Context,
    format: &str,
    corpus_id: i64,
    commit: &str,
    counters: &mut Counters,
) -> crate::Result<()> {
    let mut binary = false;
    if select::skip_read(&entry.path, ctx) || entry.size > MAX_READ {
        counters.skipped += 1;
    } else if let Some(known) = blob_binary(tx, format, &entry.oid)? {
        counters.reused += 1;
        binary = known;
    } else {
        match reader.read(&entry.oid)? {
            Some(data) => {
                counters.read += 1;
                binary = store_blob(tx, format, entry, &data, counters)?;
            }
            None => counters.skipped += 1,
        }
    }
    let role = select::role(&entry.path, ctx, binary);
    tx.prepare_cached(
        "INSERT OR REPLACE INTO file (corpus_id, \"commit\", path, path_hex, oid, mode, size, language, role, licence) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'unknown')",
    )
    .map_err(sql)?
    .execute(params![
        corpus_id,
        commit,
        entry.path,
        entry.path_hex,
        entry.oid,
        entry.mode,
        entry.size as i64,
        select::language(&entry.path),
        role.as_str()
    ])
    .map_err(sql)?;
    Ok(())
}

fn refresh_roles(
    tx: &Transaction,
    ctx: &select::Context,
    format: &str,
    corpus_id: i64,
) -> crate::Result<()> {
    let mut stmt = tx
        .prepare(
            "SELECT f.path, f.role, COALESCE(b.is_binary, 0) \
             FROM file f LEFT JOIN blob b ON b.object_format = ?1 AND b.oid = f.oid \
             WHERE f.corpus_id = ?2",
        )
        .map_err(sql)?;
    let rows: Vec<(String, String, i64)> = stmt
        .query_map(params![format, corpus_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .map_err(sql)?
        .collect::<Result<_, _>>()
        .map_err(sql)?;
    for (path, role, binary) in rows {
        let want = select::role(&path, ctx, binary != 0).as_str();
        if want != role {
            tx.execute(
                "UPDATE file SET role = ?1 WHERE corpus_id = ?2 AND path = ?3",
                params![want, corpus_id, path],
            )
            .map_err(sql)?;
        }
    }
    Ok(())
}

fn refresh_licences(tx: &Transaction, format: &str, corpus_id: i64) -> crate::Result<()> {
    let mut stmt = tx
        .prepare(
            "SELECT f.path, b.spdx_header, b.spdx_doc \
             FROM file f LEFT JOIN blob b ON b.object_format = ?1 AND b.oid = f.oid \
             WHERE f.corpus_id = ?2 ORDER BY f.path",
        )
        .map_err(sql)?;
    let rows: Vec<(String, Option<String>, Option<String>)> = stmt
        .query_map(params![format, corpus_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .map_err(sql)?
        .collect::<Result<_, _>>()
        .map_err(sql)?;
    let mut index = licence::Index::new();
    for (path, _, doc) in &rows {
        if let Some(spdx) = doc {
            if licence::is_licence_file(path) {
                index.add_licence_file(path, spdx);
            } else if licence::is_manifest(path) {
                index.add_manifest(path, spdx);
            }
        }
    }
    tx.execute("DELETE FROM licence WHERE corpus_id = ?1", params![corpus_id])
        .map_err(sql)?;
    for (path, header, _) in &rows {
        let found = index.resolve(path, header.as_deref());
        tx.prepare_cached("UPDATE file SET licence = ?1 WHERE corpus_id = ?2 AND path = ?3")
            .map_err(sql)?
            .execute(params![found.spdx, corpus_id, path])
            .map_err(sql)?;
        tx.prepare_cached(
            "INSERT INTO licence (corpus_id, path, spdx, source) VALUES (?1, ?2, ?3, ?4)",
        )
        .map_err(sql)?
        .execute(params![corpus_id, path, found.spdx, found.source])
        .map_err(sql)?;
    }
    Ok(())
}

fn index(
    store: &mut Store,
    repo: &Repo,
    name: &str,
    existing: Option<Corpus>,
    rev: &str,
) -> crate::Result<(Corpus, IndexRun)> {
    let started = Instant::now();
    let commit = repo.rev_parse(rev)?;
    let format = repo.object_format()?;
    let remote = repo.remote_url();
    let top = repo.toplevel().to_string_lossy().to_string();
    let listing = repo.ls_tree(&commit)?;
    let ctx = select::Context::from_paths(listing.iter().map(|e| e.path.as_str()));
    let tx = store
        .conn_mut()
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(sql)?;
    let prior_head = match &existing {
        Some(c) => tx
            .query_row("SELECT head, object_format FROM corpus WHERE id = ?1", params![c.id], |row| {
                Ok((row.get::<_, Option<String>>(0)?, row.get::<_, Option<String>>(1)?))
            })
            .ok()
            .filter(|(_, stored)| stored.as_deref() == Some(format.as_str()))
            .and_then(|(head, _)| head),
        None => None,
    };
    let (touched, removals) = plan(repo, prior_head.as_deref(), &commit);
    let now = now_iso();

    let corpus_id = match &existing {
        Some(c) => c.id,
        None => {
            tx.execute(
                "INSERT INTO corpus (name, path, remote, object_format) VALUES (?1, ?2, ?3, ?4)",
                params![name, top, remote, format],
            )
            .map_err(sql)?;
            tx.last_insert_rowid()
        }
    };

    let mut counters = Counters::default();
    let mut reader = repo.blob_reader()?;
    for entry in &listing {
        if let Some(set) = &touched {
            if !set.contains(&entry.path) {
                if select::skip_read(&entry.path, &ctx) || entry.size > MAX_READ {
                    counters.skipped += 1;
                    continue;
                }
                let stored: bool = tx
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM blob WHERE object_format = ?1 AND oid = ?2)",
                        params![format, entry.oid],
                        |row| row.get(0),
                    )
                    .map_err(sql)?;
                if stored {
                    counters.reused += 1;
                    continue;
                }
            }
        }
        process_entry(
            &tx,
            &mut reader,
            entry,
            &ctx,
            &format,
            corpus_id,
            &commit,
            &mut counters,
        )?;
    }
    drop(reader);

    let present: HashSet<&str> = listing.iter().map(|e| e.path.as_str()).collect();
    let mut gone: Vec<String> = Vec::new();
    if touched.is_some() {
        gone.extend(removals);
    } else {
        let mut stmt = tx
            .prepare("SELECT path FROM file WHERE corpus_id = ?1")
            .map_err(sql)?;
        let rows = stmt
            .query_map(params![corpus_id], |r| r.get::<_, String>(0))
            .map_err(sql)?;
        for row in rows {
            let path = row.map_err(sql)?;
            if !present.contains(path.as_str()) {
                gone.push(path);
            }
        }
    }
    for path in &gone {
        if present.contains(path.as_str()) {
            continue;
        }
        tx.execute(
            "DELETE FROM file WHERE corpus_id = ?1 AND path = ?2",
            params![corpus_id, path],
        )
        .map_err(sql)?;
    }

    tx.execute(
        "UPDATE file SET \"commit\" = ?1 WHERE corpus_id = ?2",
        params![commit, corpus_id],
    )
    .map_err(sql)?;
    refresh_roles(&tx, &ctx, &format, corpus_id)?;
    refresh_licences(&tx, &format, corpus_id)?;
    tx.execute(
        "DELETE FROM scan_finding WHERE corpus_id = ?1",
        params![corpus_id],
    )
    .map_err(sql)?;
    tx.execute(
        "INSERT INTO scan_finding (corpus_id, path, rule, line, fingerprint) \
         SELECT f.corpus_id, f.path, bf.rule, bf.line, bf.fingerprint \
         FROM file f JOIN blob_finding bf ON bf.object_format = ?1 AND bf.oid = f.oid \
         WHERE f.corpus_id = ?2",
        params![format, corpus_id],
    )
    .map_err(sql)?;
    tx.execute(
        "UPDATE corpus SET remote = ?1, object_format = ?2, head = ?3, indexed_at = ?4, \
         files = (SELECT COUNT(*) FROM file WHERE corpus_id = ?5), \
         bytes = (SELECT COALESCE(SUM(size), 0) FROM file WHERE corpus_id = ?5) \
         WHERE id = ?5",
        params![remote, format, commit, now, corpus_id],
    )
    .map_err(sql)?;

    let run = IndexRun {
        from: prior_head,
        to: commit.clone(),
        listed: listing.len() as u64,
        read: counters.read,
        reused: counters.reused,
        skipped: counters.skipped,
        secrets: counters.secrets,
        millis: started.elapsed().as_millis() as u64,
    };
    tx.execute(
        "INSERT INTO index_run (corpus_id, from_commit, to_commit, listed, read, reused, skipped, secrets, millis, started_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            corpus_id,
            run.from,
            run.to,
            run.listed as i64,
            run.read as i64,
            run.reused as i64,
            run.skipped as i64,
            run.secrets as i64,
            run.millis as i64,
            now
        ],
    )
    .map_err(sql)?;
    let query = format!("SELECT {CORPUS_COLUMNS} FROM corpus WHERE id = ?1");
    let corpus = tx
        .query_row(&query, params![corpus_id], corpus_from_row)
        .map_err(sql)?;
    tx.commit().map_err(sql)?;
    Ok((corpus, run))
}

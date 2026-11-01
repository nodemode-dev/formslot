use std::collections::{HashMap, HashSet};
use std::path::Path;

use regex::Regex;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use super::anchors::{AnchorCheck, check_anchors};
use super::declines;
use super::hashout::{self, Units};
use super::markdown::{self, BlockKind, clip_words};
use super::{NewRecord, content_hash, counts, event, insert, jaccard, sql};
use crate::git::Repo;
use crate::store::Store;

const ACTOR: &str = "ingest";

#[derive(Debug, Clone)]
pub struct Item {
    pub kind: &'static str,
    pub authority: &'static str,
    pub title: String,
    pub text: String,
    pub anchor: Option<String>,
    pub path: String,
    pub line: usize,
    pub triggers: String,
    pub valid_from: Option<String>,
}

impl Item {
    fn source(&self) -> String {
        format!("{}:{}", self.path, self.line)
    }

    fn hash(&self) -> String {
        content_hash(self.kind, self.authority, self.anchor.as_deref(), &self.title, &self.text)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct IngestReport {
    pub corpus: String,
    pub commit: String,
    pub sources: usize,
    pub items: usize,
    pub added: usize,
    pub unchanged: usize,
    pub superseded: usize,
    pub stale: usize,
    pub restored: usize,
    pub anchors: AnchorCheck,
    pub counts: Vec<(String, String, i64)>,
}

#[derive(Clone, Copy, PartialEq)]
enum Class {
    Claude,
    Agents,
    Notice,
    Hashout,
    Build,
    DevWise,
    Skill,
    Changelog,
}

fn classify(path: &str, role: &str) -> Option<Class> {
    if matches!(role, "mirror" | "vendored" | "generated" | "binary") {
        return None;
    }
    if path.split('/').any(|part| part == ".agents" || part == "node_modules") {
        return None;
    }
    let base = path.rsplit('/').next().unwrap_or(path);
    let lower = base.to_lowercase();
    if base == "CLAUDE.md" {
        return Some(Class::Claude);
    }
    if base == "AGENTS.md" {
        return Some(Class::Agents);
    }
    if lower == "notice.md" {
        return Some(Class::Notice);
    }
    if base.starts_with("HASHOUT_LOG") && base.ends_with(".md") {
        return Some(Class::Hashout);
    }
    if lower == "build_notes.md" {
        return Some(Class::Build);
    }
    if lower == "dev-wise.md" {
        return Some(Class::DevWise);
    }
    if lower == "changelog.md" {
        return Some(Class::Changelog);
    }
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() == 4 && parts[0] == ".claude" && parts[1] == "skills" && parts[3] == "SKILL.md" {
        return Some(Class::Skill);
    }
    None
}

fn rank(class: Class) -> u8 {
    match class {
        Class::Claude => 0,
        Class::Agents => 1,
        Class::Skill => 2,
        Class::Hashout => 3,
        Class::Changelog => 4,
        _ => 5,
    }
}

fn load_units(conn: &Connection, corpus_id: i64, format: &str) -> crate::Result<Units> {
    let mut units = Units::default();
    {
        let mut stmt = conn
            .prepare("SELECT path FROM file WHERE corpus_id = ?1")
            .map_err(sql)?;
        let rows = stmt.query_map(params![corpus_id], |r| r.get::<_, String>(0)).map_err(sql)?;
        for row in rows {
            let path = row.map_err(sql)?;
            let base = path.rsplit('/').next().unwrap_or(&path).to_string();
            units.by_base.entry(base).or_default().push(path.clone());
            units.files.insert(path);
        }
    }
    let mut stmt = conn
        .prepare(
            "SELECT f.path, u.qualified, u.name FROM file f \
             JOIN unit u ON u.object_format = ?2 AND u.oid = f.oid WHERE f.corpus_id = ?1",
        )
        .map_err(sql)?;
    let rows = stmt
        .query_map(params![corpus_id, format], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
        })
        .map_err(sql)?;
    for row in rows {
        let (path, qualified, name) = row.map_err(sql)?;
        units.units.entry(path).or_default().push((qualified, name));
    }
    Ok(units)
}

fn rule_items(path: &str, text: &str, authority: &'static str, only: Option<&Regex>, items_only: bool) -> Vec<Item> {
    let mut out = Vec::new();
    for block in markdown::blocks(text) {
        if block.trail.is_empty() || block.deepest_level() < 2 {
            continue;
        }
        if items_only && block.kind != BlockKind::Item {
            continue;
        }
        if let Some(pattern) = only
            && !block.under(pattern)
        {
            continue;
        }
        let title = format!("{}: {}", block.heading(), clip_words(&block.text, 9, 90));
        out.push(Item {
            kind: "rule",
            authority,
            title,
            text: block.text.clone(),
            anchor: None,
            path: path.to_string(),
            line: block.line,
            triggers: String::new(),
            valid_from: None,
        });
    }
    out
}

fn build_items(path: &str, text: &str) -> Vec<Item> {
    let decisions = Regex::new(r"(?i)^decisions?\b").expect("decisions pattern");
    let mut out = Vec::new();
    for block in markdown::blocks(text) {
        if block.kind != BlockKind::Item || !block.under(&decisions) {
            continue;
        }
        out.push(Item {
            kind: "decision",
            authority: "derived",
            title: format!("{}: {}", block.heading(), clip_words(&block.text, 9, 90)),
            text: block.text.clone(),
            anchor: None,
            path: path.to_string(),
            line: block.line,
            triggers: String::new(),
            valid_from: None,
        });
    }
    out
}

fn dev_wise_items(path: &str, text: &str) -> Vec<Item> {
    let mut out = Vec::new();
    for section in markdown::sections(text) {
        if section.level < 2 || section.body.is_empty() {
            continue;
        }
        out.push(Item {
            kind: "how-to",
            authority: "derived",
            title: section.title.clone(),
            text: section.body.clone(),
            anchor: None,
            path: path.to_string(),
            line: section.line,
            triggers: String::new(),
            valid_from: None,
        });
    }
    if out.is_empty() {
        for block in markdown::blocks(text) {
            if block.kind != BlockKind::Item {
                continue;
            }
            out.push(Item {
                kind: "how-to",
                authority: "derived",
                title: clip_words(&block.text, 9, 90),
                text: block.text.clone(),
                anchor: None,
                path: path.to_string(),
                line: block.line,
                triggers: String::new(),
                valid_from: None,
            });
        }
    }
    out
}

fn hashout_items(path: &str, text: &str, units: &Units) -> Vec<Item> {
    let mut out = Vec::new();
    for entry in hashout::parse(text, path, units) {
        let anchors: Vec<Option<String>> = if entry.anchors.is_empty() {
            vec![None]
        } else {
            entry.anchors.iter().cloned().map(Some).collect()
        };
        for anchor in anchors {
            out.push(Item {
                kind: entry.kind,
                authority: "derived",
                title: entry.title.clone(),
                text: entry.text.clone(),
                anchor,
                path: path.to_string(),
                line: entry.line,
                triggers: String::new(),
                valid_from: entry.logged.clone(),
            });
        }
    }
    out
}

struct Old {
    id: i64,
    status: String,
    title: String,
    text: String,
    anchor: Option<String>,
    source: String,
    hash: String,
}

fn stale_by_source(conn: &Connection, id: i64) -> crate::Result<bool> {
    let latest: Option<(String, String)> = conn
        .query_row(
            "SELECT action, note FROM memory_event WHERE record_id = ?1 ORDER BY id DESC LIMIT 1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(sql)?;
    Ok(matches!(latest, Some((action, note)) if action == "stale" && !note.starts_with("anchor ")))
}

fn source_path(source: &str) -> &str {
    source.rsplit_once(':').map(|(p, _)| p).unwrap_or(source)
}

#[derive(Default)]
struct Tally {
    added: usize,
    unchanged: usize,
    superseded: usize,
    stale: usize,
    restored: usize,
}

fn reconcile(conn: &Connection, corpus_id: i64, items: Vec<Item>, now: &str) -> crate::Result<Tally> {
    let mut tally = Tally::default();
    let mut groups: HashMap<(String, String), (Vec<Old>, Vec<Item>)> = HashMap::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT id, kind, status, title, text, anchor, source, content_hash FROM memory_record \
                 WHERE corpus_id = ?1 AND authority != 'agent' AND status IN ('active', 'stale') ORDER BY id",
            )
            .map_err(sql)?;
        let rows = stmt
            .query_map(params![corpus_id], |r| {
                Ok((
                    r.get::<_, String>(1)?,
                    Old {
                        id: r.get(0)?,
                        status: r.get(2)?,
                        title: r.get(3)?,
                        text: r.get(4)?,
                        anchor: r.get(5)?,
                        source: r.get(6)?,
                        hash: r.get(7)?,
                    },
                ))
            })
            .map_err(sql)?;
        for row in rows {
            let (kind, old) = row.map_err(sql)?;
            let key = (source_path(&old.source).to_string(), kind);
            groups.entry(key).or_default().0.push(old);
        }
    }
    for item in items {
        let key = (item.path.clone(), item.kind.to_string());
        groups.entry(key).or_default().1.push(item);
    }
    let mut keys: Vec<(String, String)> = groups.keys().cloned().collect();
    keys.sort();
    for key in keys {
        let (olds, news) = groups.remove(&key).unwrap_or_default();
        let mut taken: HashSet<usize> = HashSet::new();
        let mut rest: Vec<Item> = Vec::new();
        for item in news {
            let hash = item.hash();
            let found = olds
                .iter()
                .enumerate()
                .find(|(i, old)| !taken.contains(i) && old.hash == hash);
            match found {
                Some((i, old)) => {
                    taken.insert(i);
                    let source = item.source();
                    if old.source != source {
                        conn.execute(
                            "UPDATE memory_record SET source = ?1, updated_at = ?2 WHERE id = ?3",
                            params![source, now, old.id],
                        )
                        .map_err(sql)?;
                    }
                    if old.status == "stale" && stale_by_source(conn, old.id)? {
                        conn.execute(
                            "UPDATE memory_record SET status = 'active', updated_at = ?1 WHERE id = ?2",
                            params![now, old.id],
                        )
                        .map_err(sql)?;
                        event(conn, old.id, "restored", ACTOR, "the source line is present again with the same content")?;
                        tally.restored += 1;
                    } else if old.status != "stale" {
                        tally.unchanged += 1;
                    }
                }
                None => rest.push(item),
            }
        }
        for item in rest {
            let paired = olds
                .iter()
                .enumerate()
                .find(|(i, old)| !taken.contains(i) && old.title == item.title && old.anchor == item.anchor)
                .map(|(i, _)| i)
                .or_else(|| {
                    let mut best: Option<(usize, f64)> = None;
                    for (i, old) in olds.iter().enumerate() {
                        if taken.contains(&i) || old.anchor != item.anchor {
                            continue;
                        }
                        let score = jaccard(&old.text, &item.text);
                        if score >= 0.5 && best.is_none_or(|(_, s)| score > s) {
                            best = Some((i, score));
                        }
                    }
                    best.map(|(i, _)| i)
                });
            let record = NewRecord {
                kind: item.kind.to_string(),
                authority: item.authority.to_string(),
                status: "active".to_string(),
                title: item.title.clone(),
                text: item.text.clone(),
                anchor: item.anchor.clone(),
                corpus_id: Some(corpus_id),
                source: item.source(),
                triggers: item.triggers.clone(),
                valid_from: item.valid_from.clone().unwrap_or_else(|| now.to_string()),
            };
            let id = insert(conn, &record, now)?;
            match paired {
                Some(i) => {
                    taken.insert(i);
                    let old = &olds[i];
                    conn.execute(
                        "UPDATE memory_record SET status = 'superseded', valid_to = ?1, superseded_by = ?2, updated_at = ?1 \
                         WHERE id = ?3",
                        params![now, id, old.id],
                    )
                    .map_err(sql)?;
                    event(conn, old.id, "superseded", ACTOR, &format!("replaced by record {id}: the source changed"))?;
                    event(conn, id, "ingested", ACTOR, &format!("supersedes record {}", old.id))?;
                    tally.superseded += 1;
                }
                None => {
                    event(conn, id, "ingested", ACTOR, &item.source())?;
                    tally.added += 1;
                }
            }
        }
        for (i, old) in olds.iter().enumerate() {
            if taken.contains(&i) || old.status != "active" {
                continue;
            }
            conn.execute(
                "UPDATE memory_record SET status = 'stale', updated_at = ?1 WHERE id = ?2",
                params![now, old.id],
            )
            .map_err(sql)?;
            event(conn, old.id, "stale", ACTOR, &format!("the source line {} is no longer in the corpus", old.source))?;
            tally.stale += 1;
        }
    }
    Ok(tally)
}

fn gather(conn: &Connection, repo: &Repo, corpus_id: i64, units: &Units) -> crate::Result<(Vec<Item>, usize)> {
    let mut files: Vec<(Class, String, String)> = Vec::new();
    {
        let mut stmt = conn
            .prepare("SELECT path, oid, role FROM file WHERE corpus_id = ?1 ORDER BY path")
            .map_err(sql)?;
        let rows = stmt
            .query_map(params![corpus_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
            })
            .map_err(sql)?;
        for row in rows {
            let (path, oid, role) = row.map_err(sql)?;
            if let Some(class) = classify(&path, &role) {
                files.push((class, path, oid));
            }
        }
    }
    let mut reader = repo.blob_reader()?;
    let mut texts: Vec<(Class, String, String)> = Vec::new();
    for (class, path, oid) in files {
        if let Some(bytes) = reader.read(&oid)? {
            texts.push((class, path, String::from_utf8_lossy(&bytes).to_string()));
        }
    }
    let laws = Regex::new(r"(?i)\blaws?\b").expect("laws pattern");
    let notice = Regex::new(r"(?i)keeping|convention|current|trademark").expect("notice pattern");
    let mut items: Vec<Item> = Vec::new();
    for (class, path, text) in &texts {
        match class {
            Class::Claude | Class::Agents => items.extend(rule_items(path, text, "owner", None, false)),
            Class::Skill => items.extend(rule_items(path, text, "owner", Some(&laws), true)),
            Class::Notice => items.extend(rule_items(path, text, "owner", Some(&notice), false)),
            Class::Hashout => items.extend(hashout_items(path, text, units)),
            Class::Build => items.extend(build_items(path, text)),
            Class::DevWise => items.extend(dev_wise_items(path, text)),
            Class::Changelog => {}
        }
    }
    let mut scan: Vec<(u8, String, String)> = texts
        .iter()
        .filter(|(class, _, _)| rank(*class) < 5)
        .map(|(class, path, text)| (rank(*class), path.clone(), text.clone()))
        .collect();
    scan.sort_by(|a, b| (a.0, a.1.matches('/').count(), &a.1).cmp(&(b.0, b.1.matches('/').count(), &b.1)));
    let ordered: Vec<(String, String)> = scan.into_iter().map(|(_, p, t)| (p, t)).collect();
    for hit in declines::scan(&ordered) {
        items.push(Item {
            kind: "declined",
            authority: "owner",
            title: hit.title,
            text: hit.text,
            anchor: None,
            path: hit.path,
            line: hit.line,
            triggers: hit.triggers,
            valid_from: None,
        });
    }
    Ok((items, texts.len()))
}

pub fn ingest_with(conn: &Connection, repo: &Repo, corpus_id: i64) -> crate::Result<IngestReport> {
    let (name, format, head): (String, String, Option<String>) = conn
        .query_row(
            "SELECT name, object_format, head FROM corpus WHERE id = ?1",
            params![corpus_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(sql)?;
    conn.execute_batch("SAVEPOINT memory_ingest").map_err(sql)?;
    let outcome = (|| -> crate::Result<IngestReport> {
        let units = load_units(conn, corpus_id, &format)?;
        let (items, sources) = gather(conn, repo, corpus_id, &units)?;
        let total = items.len();
        let now = crate::corpus::now_iso();
        let first = check_anchors(conn, corpus_id)?;
        let tally = reconcile(conn, corpus_id, items, &now)?;
        let second = check_anchors(conn, corpus_id)?;
        let anchors = AnchorCheck {
            checked: second.checked,
            anchored: first.anchored + second.anchored,
            stale: first.stale + second.stale,
            restored: first.restored + second.restored,
        };
        Ok(IngestReport {
            corpus: name.clone(),
            commit: head.clone().unwrap_or_default(),
            sources,
            items: total,
            added: tally.added,
            unchanged: tally.unchanged,
            superseded: tally.superseded,
            stale: tally.stale,
            restored: tally.restored,
            anchors,
            counts: counts(conn, Some(corpus_id))?,
        })
    })();
    match outcome {
        Ok(report) => {
            conn.execute_batch("RELEASE memory_ingest").map_err(sql)?;
            Ok(report)
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK TO memory_ingest; RELEASE memory_ingest");
            Err(e)
        }
    }
}

pub fn after_index(conn: &Connection, repo: &Repo, corpus_id: i64) -> crate::Result<()> {
    ingest_with(conn, repo, corpus_id).map(|_| ())
}

pub fn ingest(store: &mut Store, key: &str) -> crate::Result<IngestReport> {
    let (corpus_id, _) = super::corpus_of(store.conn(), key)?;
    let (path, head): (String, Option<String>) = store
        .conn()
        .query_row("SELECT path, head FROM corpus WHERE id = ?1", params![corpus_id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .map_err(sql)?;
    if head.is_none() {
        return Err(format!("corpus {key} has no indexed commit yet"));
    }
    let repo = Repo::open(Path::new(&path))?;
    let tx = store.conn_mut().transaction().map_err(sql)?;
    let report = ingest_with(&tx, &repo, corpus_id)?;
    tx.commit().map_err(sql)?;
    Ok(report)
}

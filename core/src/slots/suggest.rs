use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::sync::OnceLock;
use std::time::Instant;

use regex::Regex;
use rusqlite::{Connection, params};
use serde_json::{Value, json};

use crate::corpus::{self, Corpus};
use crate::git::{BlobReader, Repo};
use crate::redact;
use crate::search::is_test_path;
use crate::slots::{manifest, storage};
use crate::store::Store;

pub const DEFAULT_LIMIT: usize = 10;
pub const DEFAULT_MIN_COPIES: usize = 2;
const MIN_DUP_LINES: i64 = 8;
const MANY_CALLERS: i64 = 8;
const MAX_FILES: usize = 8;
const MAX_COPIES: usize = 40;
const MAX_EXPORTS: usize = 40;
const MAX_CSS: usize = 60;
const SUMMARY_CHARS: usize = 240;
const DOC_CHARS: usize = 200;
const LANGUAGES: [&str; 7] = ["ts", "js", "py", "rs", "css", "sh", "wgsl"];
const NOT_SURFACE: [&str; 3] = ["selector", "custom_property", "element_id"];

#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub limit: usize,
    pub min_copies: usize,
}

#[derive(Debug, Clone)]
pub struct Suggestion {
    pub id: String,
    pub score: f64,
    pub reason: String,
    pub copies: usize,
    pub dup_lines: i64,
    pub callers: i64,
    pub manifest: Value,
}

#[derive(Debug, Clone)]
pub struct Report {
    pub corpus: String,
    pub commit: String,
    pub candidates: usize,
    pub skipped_existing: usize,
    pub invalid: Vec<String>,
    pub suggestions: Vec<Suggestion>,
    pub millis: u64,
}

fn sql(e: rusqlite::Error) -> String {
    format!("suggest: {e}")
}

struct FileInfo {
    oid: String,
    language: String,
    licence: String,
}

struct UnitRow {
    kind: String,
    name: String,
    signature: String,
    exported: bool,
    start: i64,
    end: i64,
    top: bool,
}

#[derive(Clone)]
struct Other {
    relation: String,
    score: f64,
    qualified: String,
    span: i64,
    start: i64,
    end: i64,
}

#[derive(Default)]
struct Acc {
    spans: Vec<(i64, i64)>,
    others: HashMap<(i64, String), Other>,
}

fn rank(relation: &str) -> u8 {
    match relation {
        "clone-T1" => 0,
        "clone-T2" => 1,
        "clone-T3" => 2,
        _ => 3,
    }
}

fn dir_of(path: &str) -> &str {
    path.rfind('/').map(|i| &path[..i]).unwrap_or("")
}

fn stem_of(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.split('.').next().unwrap_or(name)
}

fn record(
    accs: &mut HashMap<String, Acc>,
    files: &HashMap<String, FileInfo>,
    corpus: i64,
    path: &str,
    span: (i64, i64),
    other: (i64, &str, &str, i64, i64),
    relation: &str,
    score: f64,
) {
    if !files.contains_key(path) || (other.0 == corpus && other.1 == path) {
        return;
    }
    let acc = accs.entry(path.to_string()).or_default();
    if relation.starts_with("clone-") {
        acc.spans.push(span);
    }
    let len = (other.4 - other.3 + 1).max(1);
    let key = (other.0, other.1.to_string());
    match acc.others.get_mut(&key) {
        Some(old) => {
            if rank(relation) < rank(&old.relation) || (rank(relation) == rank(&old.relation) && score > old.score) {
                old.relation = relation.to_string();
                old.score = score;
            }
            if len > old.span {
                old.span = len;
                old.qualified = other.2.to_string();
            }
            old.start = old.start.min(other.3);
            old.end = old.end.max(other.4);
        }
        None => {
            acc.others.insert(
                key,
                Other {
                    relation: relation.to_string(),
                    score,
                    qualified: other.2.to_string(),
                    span: len,
                    start: other.3,
                    end: other.4,
                },
            );
        }
    }
}

fn gather(conn: &Connection, corpus: i64, files: &HashMap<String, FileInfo>) -> crate::Result<HashMap<String, Acc>> {
    let mut accs: HashMap<String, Acc> = HashMap::new();
    let mut stmt = conn
        .prepare(
            "SELECT m1.path, m1.start_line, m1.end_line, g.tier, m2.corpus_id, m2.path, m2.qualified, m2.start_line, m2.end_line \
             FROM clone_member m1 \
             JOIN clone_group g ON g.id = m1.group_id \
             JOIN clone_member m2 ON m2.group_id = m1.group_id \
             WHERE m1.corpus_id = ?1 AND NOT (m2.corpus_id = m1.corpus_id AND m2.path = m1.path)",
        )
        .map_err(sql)?;
    let rows = stmt
        .query_map(params![corpus], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, i64>(7)?,
                r.get::<_, i64>(8)?,
            ))
        })
        .map_err(sql)?;
    for row in rows {
        let (path, s, e, tier, oc, op, oq, os, oe) = row.map_err(sql)?;
        record(&mut accs, files, corpus, &path, (s, e), (oc, &op, &oq, os, oe), &format!("clone-{tier}"), 1.0);
    }
    for (near, far, label) in [("a", "b", "clone"), ("b", "a", "clone")] {
        let query = format!(
            "SELECT tier, score, {near}_path, {near}_start, {near}_end, {far}_corpus, {far}_path, {far}_qualified, {far}_start, {far}_end \
             FROM clone_pair WHERE {near}_corpus = ?1"
        );
        let mut stmt = conn.prepare(&query).map_err(sql)?;
        let rows = stmt
            .query_map(params![corpus], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, f64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, i64>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, String>(7)?,
                    r.get::<_, i64>(8)?,
                    r.get::<_, i64>(9)?,
                ))
            })
            .map_err(sql)?;
        for row in rows {
            let (tier, score, path, s, e, oc, op, oq, os, oe) = row.map_err(sql)?;
            record(&mut accs, files, corpus, &path, (s, e), (oc, &op, &oq, os, oe), &format!("{label}-{tier}"), score);
        }
    }
    for (near, far) in [("a", "b"), ("b", "a")] {
        let query = format!(
            "SELECT score, {near}_path, {near}_start, {near}_end, {far}_corpus, {far}_path, {far}_qualified, {far}_start, {far}_end \
             FROM related_pair WHERE reason = 'similar' AND {near}_corpus = ?1"
        );
        let mut stmt = conn.prepare(&query).map_err(sql)?;
        let rows = stmt
            .query_map(params![corpus], |r| {
                Ok((
                    r.get::<_, f64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, i64>(7)?,
                    r.get::<_, i64>(8)?,
                ))
            })
            .map_err(sql)?;
        for row in rows {
            let (score, path, s, e, oc, op, oq, os, oe) = row.map_err(sql)?;
            record(&mut accs, files, corpus, &path, (s, e), (oc, &op, &oq, os, oe), "related-similar", score);
        }
    }
    Ok(accs)
}

fn union_lines(spans: &[(i64, i64)]) -> i64 {
    let mut sorted: Vec<(i64, i64)> = spans.to_vec();
    sorted.sort();
    let mut total = 0;
    let mut at: Option<(i64, i64)> = None;
    for (s, e) in sorted {
        match at {
            Some((cs, ce)) if s <= ce + 1 => at = Some((cs, ce.max(e))),
            Some((cs, ce)) => {
                total += ce - cs + 1;
                at = Some((s, e));
            }
            None => at = Some((s, e)),
        }
    }
    if let Some((cs, ce)) = at {
        total += ce - cs + 1;
    }
    total
}

fn units_of(conn: &Connection, corpus: i64, format: &str) -> crate::Result<HashMap<String, Vec<UnitRow>>> {
    let mut stmt = conn
        .prepare(
            "SELECT f.path, u.kind, u.name, u.signature, u.exported, u.start_line, u.end_line, u.parent_idx \
             FROM file f JOIN unit u ON u.object_format = ?2 AND u.oid = f.oid \
             WHERE f.corpus_id = ?1 AND f.role = 'source' ORDER BY f.path, u.start_line, u.idx",
        )
        .map_err(sql)?;
    let rows = stmt
        .query_map(params![corpus, format], |r| {
            Ok((
                r.get::<_, String>(0)?,
                UnitRow {
                    kind: r.get(1)?,
                    name: r.get(2)?,
                    signature: r.get(3)?,
                    exported: r.get::<_, i64>(4)? != 0,
                    start: r.get(5)?,
                    end: r.get(6)?,
                    top: r.get::<_, Option<i64>>(7)?.is_none(),
                },
            ))
        })
        .map_err(sql)?;
    let mut out: HashMap<String, Vec<UnitRow>> = HashMap::new();
    for row in rows {
        let (path, unit) = row.map_err(sql)?;
        out.entry(path).or_default().push(unit);
    }
    Ok(out)
}

fn callers_of(conn: &Connection, corpus: i64) -> crate::Result<HashMap<String, i64>> {
    let mut stmt = conn
        .prepare(
            "SELECT to_path, COUNT(DISTINCT from_path) FROM edge \
             WHERE corpus_id = ?1 AND to_path <> from_path AND to_path <> '' GROUP BY to_path",
        )
        .map_err(sql)?;
    let rows = stmt
        .query_map(params![corpus], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
        .map_err(sql)?;
    let mut out = HashMap::new();
    for row in rows {
        let (path, n) = row.map_err(sql)?;
        out.insert(path, n);
    }
    Ok(out)
}

struct Existing {
    ids: HashSet<String>,
    origin: HashSet<(String, String)>,
    copies: HashSet<(String, String)>,
}

fn existing(conn: &Connection) -> crate::Result<Existing> {
    let mut found = Existing { ids: HashSet::new(), origin: HashSet::new(), copies: HashSet::new() };
    for row in storage::list(conn, None)? {
        found.ids.insert(row.id.clone());
        let repo = manifest::text_at(&row.manifest, &["origin", "repo"]).unwrap_or("").to_string();
        for file in manifest::files_of(&row.manifest) {
            found.origin.insert((repo.clone(), file.path));
        }
        if let Some(list) = row.manifest.get("copies").and_then(Value::as_array) {
            for copy in list {
                let relation = copy.get("relation").and_then(Value::as_str).unwrap_or("");
                if relation.starts_with("clone-") || relation == "related-similar" {
                    if let (Some(r), Some(p)) = (copy.get("repo").and_then(Value::as_str), copy.get("path").and_then(Value::as_str)) {
                        found.copies.insert((r.to_string(), p.to_string()));
                    }
                }
            }
        }
    }
    Ok(found)
}

struct Cand {
    path: String,
    dup: i64,
    callers: i64,
    score_part: f64,
}

struct Cluster {
    files: Vec<Cand>,
}

fn is_style(language: &str) -> bool {
    language == "css"
}

fn kebab(text: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    let trimmed = out.trim_matches('-').trim_start_matches(|c: char| !c.is_ascii_lowercase());
    trimmed.to_string()
}

fn humanise(name: &str) -> String {
    let words = name.replace(['-', '_'], " ");
    let mut chars = words.trim().chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn name_of(files: &[String]) -> String {
    let first = &files[0];
    let stem = stem_of(first);
    let generic = ["index", "mod", "lib", "main", "__init__", "init", "utils", "util"];
    let pick = if files.len() > 1 || generic.contains(&stem) {
        let dir = dir_of(first);
        let mut segs = dir.rsplit('/');
        let last = segs.next().unwrap_or("");
        let plain = ["core", "src", "lib", "app", "utils", "util", "common", "shared", "routers", "io", "ui"];
        if last.is_empty() {
            stem.to_string()
        } else if plain.contains(&last) {
            match segs.next() {
                Some(up) if !up.is_empty() && !plain[1..3].contains(&up) => format!("{up}-{last}"),
                _ => last.to_string(),
            }
        } else {
            last.to_string()
        }
    } else {
        stem.to_string()
    };
    let k = kebab(&pick);
    if k.is_empty() { "unit".to_string() } else { k }
}

fn classify(path: &str, language: &str) -> (&'static str, &'static str) {
    let lower = path.to_ascii_lowercase();
    if is_style(language) {
        return ("css", "style");
    }
    if language == "sh" {
        return ("script", "script");
    }
    if lower.contains("src-tauri/") {
        return ("tauri", "tauri-command");
    }
    let service = ["service", "server", "mcp", "routes", "router", "api", "backend", "bridge"];
    if language == "py" || language == "rs" {
        if service.iter().any(|s| lower.contains(s)) {
            return ("svc", "service");
        }
        return ("lib", "module");
    }
    if lower.contains("/ui/") || lower.contains("component") || lower.ends_with(".tsx") {
        return ("ui", "component");
    }
    ("lib", "module")
}

fn layer_of(path: &str, language: &str) -> Option<&'static str> {
    let lower = path.to_ascii_lowercase();
    if lower.contains("src-tauri/") || lower.starts_with("cli/") {
        return Some("shell");
    }
    if lower.starts_with("frontend/") || matches!(language, "css" | "ts" | "js") {
        return Some("frontend");
    }
    if lower.starts_with("backend/") || lower.starts_with("core/") || matches!(language, "py" | "rs") {
        return Some("engine");
    }
    None
}

fn fit(text: &str, max: usize) -> String {
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        return flat;
    }
    let cut: String = flat.chars().take(max.saturating_sub(3)).collect();
    format!("{}...", cut.trim_end())
}

fn strip_marks(line: &str) -> String {
    let t = line.trim();
    let t = t.trim_start_matches(['/', '*', '#', '!', '-']).trim_end_matches(['*', '/']);
    t.trim().trim_matches(['"', '\'']).trim().to_string()
}

fn is_comment_line(line: &str) -> bool {
    let t = line.trim_start();
    if t.starts_with("#[") || t.starts_with("#!") {
        return false;
    }
    t.starts_with("//") || t.starts_with("/*") || t.starts_with('*') || t.starts_with('#') || t.starts_with("--")
}

fn is_decoration(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("#[") || t.starts_with('@')
}

fn unit_doc(lines: &[&str], start: i64, language: &str) -> String {
    let at = (start - 1).max(0) as usize;
    let mut parts: Vec<String> = Vec::new();
    let mut i = at;
    while i > 0 {
        let line = lines[i - 1];
        if is_decoration(line) {
            i -= 1;
            continue;
        }
        if is_comment_line(line) {
            let s = strip_marks(line);
            if !s.is_empty() {
                parts.push(s);
            }
            i -= 1;
            continue;
        }
        break;
    }
    parts.reverse();
    if parts.is_empty() && language == "py" {
        if let Some(next) = lines.get(at + 1) {
            let t = next.trim();
            if t.starts_with("\"\"\"") || t.starts_with("'''") {
                let s = strip_marks(t);
                if !s.is_empty() {
                    parts.push(s);
                }
            }
        }
    }
    fit(&parts.join(" "), DOC_CHARS)
}

fn header_doc(lines: &[&str]) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut started = false;
    for (n, line) in lines.iter().enumerate() {
        let t = line.trim();
        if n == 0 && t.starts_with("#!") {
            continue;
        }
        if t.is_empty() {
            if started {
                break;
            }
            continue;
        }
        if t.starts_with("\"\"\"") || t.starts_with("'''") {
            let s = strip_marks(t);
            if !s.is_empty() {
                parts.push(s);
            }
            break;
        }
        if is_comment_line(t) {
            started = true;
            let s = strip_marks(t);
            if !s.is_empty() {
                parts.push(s);
            }
        } else {
            break;
        }
    }
    let joined = parts.join(" ");
    let lower = joined.to_ascii_lowercase();
    if lower.contains("copyright") || lower.contains("spdx") || lower.contains("licen") {
        return String::new();
    }
    fit(&joined, SUMMARY_CHARS)
}

fn first_lines(lines: &[&str]) -> String {
    let skip = ["import ", "from ", "use ", "export {", "export *", "#include", "'use strict'", "\"use strict\"", "mod ", "pub mod ", "pub use ", "} from", "})", "{", "}"];
    let picked: Vec<&str> = lines
        .iter()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !is_comment_line(l) && !skip.iter().any(|s| l.starts_with(s)) && !l.starts_with('.') && !l.ends_with(" from '") && l.chars().count() > 8)
        .take(2)
        .collect();
    fit(&picked.join(" "), SUMMARY_CHARS)
}

fn css_names(units: &[UnitRow]) -> (Vec<String>, Vec<String>) {
    let mut classes: BTreeSet<String> = BTreeSet::new();
    let mut vars: BTreeSet<String> = BTreeSet::new();
    for u in units {
        if u.kind == "selector" {
            for part in u.name.split(',') {
                let p = part.trim();
                if let Some(rest) = p.strip_prefix('.') {
                    let name: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
                    if !name.is_empty() {
                        classes.insert(name);
                    }
                }
            }
        } else if u.kind == "custom_property" {
            vars.insert(u.name.clone());
        }
    }
    (classes.into_iter().take(MAX_CSS).collect(), vars.into_iter().take(MAX_CSS).collect())
}

fn app_version(conn: &Connection, corpus: &Corpus, reader: &mut BlobReader) -> String {
    static JSON: OnceLock<Regex> = OnceLock::new();
    static TOML: OnceLock<Regex> = OnceLock::new();
    let json = JSON.get_or_init(|| Regex::new(r#""version"\s*:\s*"(\d+\.\d+\.\d+[^"]*)""#).expect("fixed pattern"));
    let toml = TOML.get_or_init(|| Regex::new(r#"(?m)^version\s*=\s*"(\d+\.\d+\.\d+[^"]*)""#).expect("fixed pattern"));
    for path in ["package.json", "frontend/package.json", "Cargo.toml", "backend/pyproject.toml", "pyproject.toml"] {
        let oid: Option<String> = conn
            .query_row("SELECT oid FROM file WHERE corpus_id = ?1 AND path = ?2", params![corpus.id, path], |r| r.get(0))
            .ok();
        let Some(oid) = oid else { continue };
        let Ok(Some(bytes)) = reader.read(&oid) else { continue };
        let text = String::from_utf8_lossy(&bytes);
        let re = if path.ends_with(".json") { json } else { toml };
        if let Some(c) = re.captures(&text) {
            return c[1].to_string();
        }
    }
    corpus.head.as_deref().map(|h| h.chars().take(12).collect()).unwrap_or_else(|| "0.0.0".to_string())
}

struct Readers<'a> {
    conn: &'a Connection,
    corpora: &'a HashMap<i64, Corpus>,
    open: HashMap<i64, Option<BlobReader>>,
}

impl<'a> Readers<'a> {
    fn hash(&mut self, corpus: i64, path: &str) -> Option<(String, i64)> {
        let oid: String = self
            .conn
            .query_row("SELECT oid FROM file WHERE corpus_id = ?1 AND path = ?2", params![corpus, path], |r| r.get(0))
            .ok()?;
        let reader = self
            .open
            .entry(corpus)
            .or_insert_with(|| {
                let c = self.corpora.get(&corpus)?;
                Repo::open(Path::new(&c.path)).ok()?.blob_reader().ok()
            })
            .as_mut()?;
        let bytes = reader.read(&oid).ok()??;
        Some(manifest::hash_text(&bytes))
    }
}

fn flagged(conn: &Connection, corpus: i64, path: &str, format: &str, oid: &str) -> bool {
    let count = |q: &str, a: &dyn rusqlite::ToSql, b: &dyn rusqlite::ToSql| -> i64 {
        conn.query_row(q, params![a, b], |r| r.get(0)).unwrap_or(0)
    };
    count("SELECT COUNT(*) FROM scan_finding WHERE corpus_id = ?1 AND path = ?2", &corpus, &path) > 0
        || count("SELECT COUNT(*) FROM blob_finding WHERE object_format = ?1 AND oid = ?2", &format, &oid) > 0
}

pub fn suggest(store: &Store, corpus_key: &str, opts: Options) -> crate::Result<Report> {
    let started = Instant::now();
    let conn = store.conn();
    let all = corpus::list(store)?;
    let corpora: HashMap<i64, Corpus> = all.iter().map(|c| (c.id, c.clone())).collect();
    let target = all
        .iter()
        .find(|c| c.name == corpus_key)
        .or_else(|| all.iter().find(|c| c.id.to_string() == corpus_key))
        .cloned()
        .ok_or_else(|| format!("no corpus named {corpus_key}; formslot corpus list shows the corpora"))?;
    let head = target
        .head
        .clone()
        .ok_or_else(|| format!("corpus {} has no indexed commit yet; run formslot reindex {}", target.name, target.name))?;
    if head.len() < 7 || !head.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
        return Err(format!("corpus {} has no usable head commit", target.name));
    }
    let names: HashMap<i64, String> = all.iter().map(|c| (c.id, c.name.clone())).collect();

    let mut files: HashMap<String, FileInfo> = HashMap::new();
    {
        let mut stmt = conn
            .prepare("SELECT path, oid, language, licence FROM file WHERE corpus_id = ?1 AND role = 'source'")
            .map_err(sql)?;
        let rows = stmt
            .query_map(params![target.id], |r| {
                Ok((r.get::<_, String>(0)?, FileInfo { oid: r.get(1)?, language: r.get(2)?, licence: r.get(3)? }))
            })
            .map_err(sql)?;
        for row in rows {
            let (path, info) = row.map_err(sql)?;
            if LANGUAGES.contains(&info.language.as_str()) && !is_test_path(&path) {
                files.insert(path, info);
            }
        }
    }
    let accs = gather(conn, target.id, &files)?;
    let units = units_of(conn, target.id, &target.object_format)?;
    let callers = callers_of(conn, target.id)?;
    let have = existing(conn)?;
    let min_others = opts.min_copies.saturating_sub(1);

    let mut skipped_existing = 0usize;
    let mut cands: Vec<(Cand, Option<(i64, String)>)> = Vec::new();
    let mut paths: Vec<&String> = files.keys().collect();
    paths.sort();
    for path in paths {
        let empty = Acc::default();
        let acc = accs.get(path).unwrap_or(&empty);
        let list = units.get(path).map(Vec::as_slice).unwrap_or(&[]);
        let language = files[path].language.as_str();
        let surface = if is_style(language) {
            list.iter().filter(|u| u.kind == "selector" || u.kind == "custom_property").count()
        } else {
            list.iter().filter(|u| u.top && u.exported && !NOT_SURFACE.contains(&u.kind.as_str())).count()
        };
        let dup = union_lines(&acc.spans);
        let calls = callers.get(path).copied().unwrap_or(0);
        let copy_ok = acc.others.len() >= min_others.max(1) && dup >= MIN_DUP_LINES;
        let related_ok = acc.others.len() >= min_others.max(1) && dup == 0 && acc.others.values().any(|o| o.score >= 0.6) && calls >= 3;
        if surface == 0 || !(copy_ok || related_ok || calls >= MANY_CALLERS) {
            continue;
        }
        let name = names.get(&target.id).cloned().unwrap_or_default();
        let covered = have.origin.contains(&(name.clone(), path.clone()))
            || have.copies.contains(&(name.clone(), path.clone()))
            || acc.others.keys().any(|(c, p)| names.get(c).is_some_and(|n| have.origin.contains(&(n.clone(), p.clone()))));
        if covered {
            skipped_existing += 1;
            continue;
        }
        let distinct_repos: HashSet<i64> = acc.others.keys().map(|(c, _)| *c).filter(|c| *c != target.id).collect();
        let tiers: BTreeSet<&str> = acc.others.values().map(|o| o.relation.as_str()).collect();
        let part = 2.0 * ((1 + dup) as f64).ln()
            + 3.0 * ((1 + acc.others.len()) as f64).ln()
            + 2.0 * distinct_repos.len() as f64
            + 2.5 * ((1 + calls) as f64).ln()
            + 0.5 * (surface.min(10) as f64)
            + if tiers.contains("clone-T1") { 1.0 } else { 0.0 };
        let mut by_dir: HashMap<(i64, String), i64> = HashMap::new();
        for ((c, p), o) in &acc.others {
            *by_dir.entry((*c, dir_of(p).to_string())).or_insert(0) += o.span;
        }
        let primary = by_dir.into_iter().max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0))).map(|(k, _)| k);
        cands.push((Cand { path: path.clone(), dup, callers: calls, score_part: part }, primary));
    }
    let candidates_found = cands.len();

    let mut buckets: HashMap<(String, Option<(i64, String)>), Vec<Cand>> = HashMap::new();
    let mut singles: Vec<Cluster> = Vec::new();
    for (cand, primary) in cands {
        match primary {
            Some(p) => buckets.entry((dir_of(&cand.path).to_string(), Some(p))).or_default().push(cand),
            None => singles.push(Cluster { files: vec![cand] }),
        }
    }
    let mut clusters: Vec<Cluster> = singles;
    for (_, mut members) in buckets {
        if members.len() <= MAX_FILES {
            clusters.push(Cluster { files: members });
        } else {
            for m in members.drain(..) {
                clusters.push(Cluster { files: vec![m] });
            }
        }
    }
    let mut ranked: Vec<(f64, Cluster)> = clusters
        .into_iter()
        .map(|c| {
            let total: f64 = c.files.iter().map(|f| f.score_part).fold(0.0, f64::max)
                + 0.5 * c.files.iter().skip(1).map(|f| f.score_part).sum::<f64>().min(10.0);
            (total, c)
        })
        .collect();
    ranked.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.files[0].path.cmp(&b.1.files[0].path))
    });

    let repo = Repo::open(Path::new(&target.path))?;
    let mut reader = repo.blob_reader()?;
    let app_ver = app_version(conn, &target, &mut reader);
    let mut readers = Readers { conn, corpora: &corpora, open: HashMap::new() };
    let mut taken: HashSet<String> = have.ids.clone();
    let mut out: Vec<Suggestion> = Vec::new();
    let mut invalid: Vec<String> = Vec::new();
    let want = if opts.limit == 0 { usize::MAX } else { opts.limit };
    for (score, cluster) in ranked {
        if out.len() >= want {
            break;
        }
        let mut members: Vec<&Cand> = cluster.files.iter().collect();
        members.sort_by(|a, b| a.path.cmp(&b.path));
        let mut entries: Vec<Value> = Vec::new();
        let mut texts: Vec<(String, String, String)> = Vec::new();
        let mut kept: Vec<&Cand> = Vec::new();
        let mut pairs: Vec<(String, String)> = Vec::new();
        for m in &members {
            let info = &files[&m.path];
            if flagged(conn, target.id, &m.path, &target.object_format, &info.oid) {
                continue;
            }
            let Ok(Some(bytes)) = reader.read(&info.oid) else { continue };
            if bytes.contains(&0) {
                continue;
            }
            let (sha, lines) = manifest::hash_text(&bytes);
            let role = if is_style(&info.language) { "style" } else { "source" };
            entries.push(json!({ "path": m.path, "role": role, "sha256": sha, "lines": lines, "port": "adapt" }));
            pairs.push((m.path.clone(), sha));
            texts.push((m.path.clone(), info.language.clone(), String::from_utf8_lossy(&bytes).to_string()));
            kept.push(m);
        }
        if kept.is_empty() {
            continue;
        }
        let primary = kept.iter().max_by(|a, b| a.dup.cmp(&b.dup).then(a.callers.cmp(&b.callers)).then(b.path.cmp(&a.path))).copied().unwrap_or(kept[0]);
        let primary_info = &files[&primary.path];
        let paths_kept: Vec<String> = kept.iter().map(|k| k.path.clone()).collect();
        let (prefix, kind) = classify(&primary.path, &primary_info.language);
        let name = name_of(&paths_kept);
        let mut id = format!("{prefix}.{name}");
        if taken.contains(&id) {
            id = format!("{prefix}.{name}-{}", kebab(&target.name));
        }
        let base = id.clone();
        let mut n = 2;
        while taken.contains(&id) {
            id = format!("{base}-{n}");
            n += 1;
        }
        if id.len() > 80 {
            id.truncate(80);
            id = id.trim_end_matches(['-', '.']).to_string();
        }
        taken.insert(id.clone());

        let mut exports: Vec<Value> = Vec::new();
        let mut css_classes: Vec<String> = Vec::new();
        let mut css_vars: Vec<String> = Vec::new();
        let mut best_doc = (0i64, String::new());
        let mut head_doc = String::new();
        let mut fallback = String::new();
        for (path, language, text) in &texts {
            let lines: Vec<&str> = text.lines().collect();
            let list = units.get(path).map(Vec::as_slice).unwrap_or(&[]);
            if is_style(language) {
                let (c, v) = css_names(list);
                css_classes.extend(c);
                css_vars.extend(v);
            }
            for u in list.iter().filter(|u| u.top && u.exported && !NOT_SURFACE.contains(&u.kind.as_str())) {
                let doc = unit_doc(&lines, u.start, language);
                let size = u.end - u.start + 1;
                if !doc.is_empty() && size > best_doc.0 {
                    best_doc = (size, doc.clone());
                }
                if exports.len() < MAX_EXPORTS {
                    let sig = redact::text(&fit(&u.signature, 200)).0;
                    let mut e = json!({ "file": path, "symbol": u.name, "signature": sig });
                    if !doc.is_empty() {
                        e["doc"] = json!(redact::text(&doc).0);
                    }
                    exports.push(e);
                }
            }
            if *path == primary.path {
                head_doc = header_doc(&lines);
                fallback = first_lines(&lines);
            }
        }
        css_classes.sort();
        css_classes.dedup();
        css_vars.sort();
        css_vars.dedup();
        let summary_raw = if !head_doc.is_empty() {
            head_doc
        } else if !best_doc.1.is_empty() {
            best_doc.1.clone()
        } else if !exports.is_empty() {
            let symbols: Vec<&str> = exports.iter().filter_map(|e| e["symbol"].as_str()).take(6).collect();
            format!("{} exporting {}", humanise(&name), symbols.join(", "))
        } else if !fallback.is_empty() {
            fallback
        } else {
            format!("{} from {}", humanise(&name), primary.path)
        };
        let summary = fit(&redact::text(&summary_raw).0, SUMMARY_CHARS);

        let mut copy_src: Vec<(String, (i64, String), Other)> = Vec::new();
        let mut seen: HashSet<(i64, String)> = HashSet::new();
        for k in &kept {
            if let Some(acc) = accs.get(&k.path) {
                for (key, o) in &acc.others {
                    if seen.insert(key.clone()) {
                        copy_src.push((k.path.clone(), key.clone(), o.clone()));
                    }
                }
            }
        }
        copy_src.sort_by(|a, b| {
            rank(&a.2.relation)
                .cmp(&rank(&b.2.relation))
                .then(b.2.score.partial_cmp(&a.2.score).unwrap_or(std::cmp::Ordering::Equal))
                .then(b.2.span.cmp(&a.2.span))
                .then(names.get(&a.1.0).cmp(&names.get(&b.1.0)))
                .then(a.1.1.cmp(&b.1.1))
        });
        let total_copies = copy_src.len();
        let repos: BTreeSet<String> = copy_src.iter().map(|c| names.get(&c.1.0).cloned().unwrap_or_default()).collect();
        let mut copies: Vec<Value> = Vec::new();
        for (_, (cid, cpath), o) in copy_src.into_iter().take(MAX_COPIES) {
            let mut c = json!({
                "repo": names.get(&cid).cloned().unwrap_or_else(|| cid.to_string()),
                "path": cpath,
                "qualified": o.qualified,
                "lines": [o.start.max(1), o.end.max(o.start).max(1)],
                "relation": o.relation,
                "score": (o.score.clamp(0.0, 1.0) * 1000.0).round() / 1000.0,
            });
            if let Some((sha, _)) = readers.hash(cid, &cpath) {
                c["sha256"] = json!(sha);
            }
            copies.push(c);
        }

        let dup: i64 = kept.iter().map(|k| k.dup).sum();
        let calls: i64 = kept.iter().map(|k| k.callers).sum();
        let tier_names: BTreeSet<String> = copies
            .iter()
            .filter_map(|c| c["relation"].as_str())
            .map(|r| r.trim_start_matches("clone-").trim_start_matches("related-").to_string())
            .collect();
        let mut parts: Vec<String> = Vec::new();
        if total_copies > 0 {
            let shown: Vec<String> = repos.iter().take(5).cloned().collect();
            parts.push(format!(
                "{} other cop{} in {} ({})",
                total_copies,
                if total_copies == 1 { "y" } else { "ies" },
                if repos.len() == 1 { "1 repo".to_string() } else { format!("{} repos", repos.len()) },
                shown.join(", ")
            ));
        }
        if dup > 0 {
            parts.push(format!("{} duplicated lines ({})", dup, tier_names.iter().cloned().collect::<Vec<_>>().join(", ")));
        } else if total_copies > 0 {
            parts.push(format!("related only ({})", tier_names.iter().cloned().collect::<Vec<_>>().join(", ")));
        }
        if calls > 0 {
            parts.push(format!("used from {calls} file{}", if calls == 1 { "" } else { "s" }));
        }
        let reason = parts.join("; ");

        let (origin_path, _) = if kept.len() == 1 {
            (kept[0].path.clone(), 0)
        } else {
            let d = dir_of(&kept[0].path);
            (if d.is_empty() { kept[0].path.clone() } else { d.to_string() }, 0)
        };
        let content_hash = manifest::content_hash(&pairs);
        let mut api = json!({ "exports": exports });
        if !css_classes.is_empty() {
            api["cssClasses"] = json!(css_classes);
        }
        if !css_vars.is_empty() {
            api["cssVars"] = json!(css_vars);
        }
        let licence = if primary_info.licence.is_empty() || primary_info.licence == "unknown" { None } else { Some(primary_info.licence.clone()) };
        let mut m = json!({
            "schemaVersion": 1,
            "id": id,
            "title": fit(&humanise(&name), 120),
            "summary": summary,
            "description": format!("Drafted by formslot slot suggest from {} at {}: {}. Review before proposing.", target.name, &head[..12.min(head.len())], reason),
            "kind": kind,
            "tags": ["suggested"],
            "status": "proposed",
            "version": "0.1.0",
            "origin": {
                "repo": target.name,
                "path": origin_path,
                "commit": head,
                "app": target.name,
                "appVersion": app_ver,
            },
            "api": api,
            "files": entries,
            "contentHash": content_hash,
            "copies": copies,
            "meta": {
                "suggest": {
                    "corpus": target.name,
                    "commit": head,
                    "score": (score * 100.0).round() / 100.0,
                    "reason": reason,
                    "copies": total_copies,
                    "dupLines": dup,
                    "callers": calls,
                }
            }
        });
        if let Some(layer) = layer_of(&primary.path, &primary_info.language) {
            m["layer"] = json!(layer);
        }
        if let Some(l) = licence {
            m["license"] = json!(l);
        }
        let problems = manifest::validate(&m);
        if !problems.is_empty() {
            invalid.push(format!("{id}: {}", problems.join("; ")));
            continue;
        }
        out.push(Suggestion { id, score, reason, copies: total_copies, dup_lines: dup, callers: calls, manifest: m });
    }
    Ok(Report {
        corpus: target.name,
        commit: head,
        candidates: candidates_found,
        skipped_existing,
        invalid,
        suggestions: out,
        millis: started.elapsed().as_millis() as u64,
    })
}

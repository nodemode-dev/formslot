use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use rusqlite::{Connection, OpenFlags, params};
use serde::Serialize;
use serde_json::{Value, json};

use crate::memory::{self, Filter, Record};
use crate::search::index as search_index;
use crate::search::lexical;
use crate::search::text::{split_words, stem};
use crate::search::{Target, is_test_path, targets};
use crate::select;
use crate::slots::storage;
use crate::symbols;

pub const BUDGET: Duration = Duration::from_millis(900);
const READ_CAP: u64 = 1_048_576;
const TEXT_CAP: usize = 400_000;
const STRONG: f64 = 2.5;
const MAX_SLOTS: usize = 3;
const MAX_UNITS: usize = 2;
const QUOTE_CAP: usize = 500;
const DOC_EXTENSIONS: [&str; 5] = ["md", "mdx", "txt", "rst", "adoc"];
const DOC_NAMES: [&str; 6] = ["claude", "agents", "notice", "changelog", "hashout_log", "readme"];
const GENERIC: [&str; 31] = [
    "index", "main", "util", "utils", "helper", "helpers", "type", "types", "test", "tests", "spec", "src", "lib",
    "ts", "tsx", "js", "jsx", "py", "rs", "new", "get", "set", "make", "create", "build", "init", "handle", "app",
    "frontend", "backend", "ui",
];
const REUSE_KINDS: [&str; 9] = ["function", "class", "const", "constant", "struct", "trait", "interface", "type", "enum"];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Warn,
    Deny,
    Off,
}

impl Mode {
    pub fn parse(text: &str) -> Option<Mode> {
        match text.trim().to_lowercase().as_str() {
            "warn" => Some(Mode::Warn),
            "deny" => Some(Mode::Deny),
            "off" => Some(Mode::Off),
            _ => None,
        }
    }

    pub fn resolve(arg: Option<&str>) -> Mode {
        if let Some(mode) = arg.and_then(Mode::parse) {
            return mode;
        }
        std::env::var("FORMSLOT_HOOK_MODE").ok().and_then(|v| Mode::parse(&v)).unwrap_or(Mode::Warn)
    }
}

#[derive(Debug, Clone)]
pub struct Request {
    pub tool: String,
    pub path: String,
    pub new_text: String,
    pub old_text: Option<String>,
    pub cwd: String,
    pub session: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SlotRef {
    pub id: String,
    pub status: String,
    pub path: String,
    pub line: Option<i64>,
    pub symbol: Option<String>,
    pub copy: Option<String>,
    pub score: f64,
    pub matched: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum Verdict {
    Allow,
    Warn { message: String, slots: Vec<SlotRef> },
    Deny { message: String },
}

pub fn request_of(v: &Value) -> Option<Request> {
    let tool = v.get("tool_name")?.as_str()?.to_string();
    let input = v.get("tool_input")?;
    let cwd = v.get("cwd").and_then(Value::as_str).unwrap_or("").to_string();
    let session = v.get("session_id").and_then(Value::as_str).unwrap_or("").to_string();
    let raw = input.get("file_path").or_else(|| input.get("notebook_path")).and_then(Value::as_str)?.to_string();
    let path = if Path::new(&raw).is_absolute() || cwd.is_empty() {
        clean(Path::new(&raw)).to_string_lossy().to_string()
    } else {
        clean(&Path::new(&cwd).join(&raw)).to_string_lossy().to_string()
    };
    let (new_text, old_text) = match tool.as_str() {
        "Write" => (input.get("content")?.as_str()?.to_string(), None),
        "NotebookEdit" => (input.get("new_source")?.as_str()?.to_string(), None),
        "Edit" => (
            input.get("new_string")?.as_str()?.to_string(),
            Some(input.get("old_string").and_then(Value::as_str).unwrap_or("").to_string()),
        ),
        "MultiEdit" => {
            let edits = input.get("edits")?.as_array()?;
            let news: Vec<&str> = edits.iter().filter_map(|e| e.get("new_string").and_then(Value::as_str)).collect();
            let olds: Vec<&str> = edits.iter().filter_map(|e| e.get("old_string").and_then(Value::as_str)).collect();
            (news.join("\n"), Some(olds.join("\n")))
        }
        _ => return None,
    };
    Some(Request { tool, path, new_text, old_text, cwd, session })
}

fn partial_string(text: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let mut from = 0;
    while let Some(found) = text.get(from..).and_then(|t| t.find(&needle)) {
        let at = from + found + needle.len();
        from = at;
        let Some(rest) = text[at..].trim_start().strip_prefix(':') else { continue };
        let Some(body) = rest.trim_start().strip_prefix('"') else { continue };
        let bytes = body.as_bytes();
        let mut i = 0;
        let mut end = None;
        while i < bytes.len() {
            match bytes[i] {
                b'\\' => i += 2,
                b'"' => {
                    end = Some(i);
                    break;
                }
                _ => i += 1,
            }
        }
        let mut raw = &body[..end.unwrap_or(body.len())];
        loop {
            let err = match serde_json::from_str::<String>(&format!("\"{raw}\"")) {
                Ok(decoded) => return Some(decoded),
                Err(e) => e,
            };
            let mut cut = raw.len().saturating_sub(1);
            if err.line() == 1 {
                cut = cut.min(err.column().saturating_sub(2));
            }
            while cut > 0 && !raw.is_char_boundary(cut) {
                cut -= 1;
            }
            if let Some(slash) = raw[..cut].rfind('\\') {
                cut = slash;
            }
            raw = &raw[..cut];
        }
    }
    None
}

pub fn lenient(text: &str) -> Option<Value> {
    let tool = partial_string(text, "tool_name")?;
    let mut input = serde_json::Map::new();
    for key in ["file_path", "notebook_path", "content", "new_source", "old_string", "new_string"] {
        if let Some(v) = partial_string(text, key) {
            input.insert(key.to_string(), Value::String(v));
        }
    }
    if tool == "MultiEdit" {
        let edit = json!({
            "old_string": input.remove("old_string").unwrap_or(Value::String(String::new())),
            "new_string": input.remove("new_string").unwrap_or(Value::String(String::new())),
        });
        input.insert("edits".to_string(), json!([edit]));
    }
    Some(json!({
        "tool_name": tool,
        "cwd": partial_string(text, "cwd").unwrap_or_default(),
        "session_id": partial_string(text, "session_id").unwrap_or_default(),
        "tool_input": Value::Object(input),
    }))
}

pub fn open_read(path: &Path) -> crate::Result<Connection> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .map_err(|e| format!("check: {e}"))?;
    conn.busy_timeout(Duration::from_millis(300)).map_err(|e| format!("check: {e}"))?;
    Ok(conn)
}

pub fn clean(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() && !out.has_root() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

pub fn relative(corpus: &Target, path: &str) -> Option<String> {
    let cleaned = clean(Path::new(path));
    let root = Path::new(&corpus.path);
    let given = cleaned.as_path();
    let rest = given.strip_prefix(root).ok().map(Path::to_path_buf).or_else(|| {
        let canon_root = root.canonicalize().ok()?;
        let canon = given.canonicalize().ok().or_else(|| {
            let parent = given.parent()?.canonicalize().ok()?;
            Some(parent.join(given.file_name()?))
        })?;
        canon.strip_prefix(canon_root).ok().map(Path::to_path_buf)
    })?;
    let parts: Vec<String> = rest.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect();
    if parts.is_empty() { None } else { Some(parts.join("/")) }
}

pub fn corpus_for(all: &[Target], cwd: &str) -> Option<Target> {
    if cwd.is_empty() {
        return None;
    }
    let pick = |probe: &Path| -> Option<Target> {
        all.iter()
            .filter(|t| probe.starts_with(&t.path))
            .max_by_key(|t| t.path.len())
            .cloned()
    };
    pick(Path::new(cwd)).or_else(|| Path::new(cwd).canonicalize().ok().and_then(|c| pick(&c)))
}

fn norm(text: &str) -> String {
    format!(" {} ", split_words(text).join(" "))
}

fn doc_path(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel).to_lowercase();
    let (stem_part, ext) = match name.rsplit_once('.') {
        Some((s, e)) => (s.to_string(), e.to_string()),
        None => (name.clone(), String::new()),
    };
    let doc_ext = ext.is_empty() || DOC_EXTENSIONS.contains(&ext.as_str());
    DOC_EXTENSIONS.contains(&ext.as_str()) || (doc_ext && DOC_NAMES.contains(&stem_part.as_str())) || rel.starts_with("docs/")
}

pub struct Decline {
    pub record: Record,
    pub phrase: String,
    pub place: &'static str,
}

pub fn declines(conn: &Connection, corpus_id: i64) -> crate::Result<Vec<Record>> {
    let mut records = memory::list(
        conn,
        &Filter {
            kind: Some("declined".to_string()),
            status: Some("active".to_string()),
            corpus_id: Some(corpus_id),
            ..Filter::default()
        },
    )?;
    if let Ok(specs) = memory::declines::specs() {
        for record in &mut records {
            if let Some(spec) = specs.iter().find(|s| s.title == record.title) {
                record.triggers = spec.triggers.clone();
            }
        }
    }
    Ok(records)
}

pub fn find_decline(
    records: &[Record],
    rel: &str,
    new_text: &str,
    old_text: &str,
    scan_path: bool,
) -> Option<Decline> {
    let path_n = norm(rel);
    let scan_content = !doc_path(rel);
    let new_n = if scan_content { norm(new_text) } else { String::new() };
    let old_n = if scan_content { norm(old_text) } else { String::new() };
    for record in records {
        for phrase in record.triggers.split(',') {
            let p = split_words(phrase).join(" ");
            if p.is_empty() {
                continue;
            }
            let pat = format!(" {p} ");
            if scan_content && new_n.contains(&pat) && !old_n.contains(&pat) {
                return Some(Decline { record: record.clone(), phrase: phrase.trim().to_string(), place: "the new text" });
            }
            if scan_path && path_n.contains(&pat) {
                return Some(Decline { record: record.clone(), phrase: phrase.trim().to_string(), place: "the file path" });
            }
        }
    }
    None
}

fn clip(text: &str, cap: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= cap {
        return flat;
    }
    let cut: String = flat.chars().take(cap.saturating_sub(3)).collect();
    format!("{cut}...")
}

pub fn decline_message(d: &Decline, rel: &str) -> String {
    let text = crate::redact::text(&clip(&d.record.text, QUOTE_CAP)).0;
    format!(
        "formslot: blocked, the owner declined this. Rule: \"{text}\" ({}; owner record {}, source {}). The trigger \"{}\" is in {} of {rel}. Ask the owner before adding it; `formslot recall \"{}\"` shows the record.",
        clip(&d.record.title, 160),
        d.record.id,
        d.record.source,
        d.phrase,
        d.place,
        d.phrase
    )
}

fn canon(word: &str) -> &str {
    match word {
        "colour" => "color",
        "colours" => "colors",
        "centre" => "center",
        "grey" => "gray",
        "behaviour" => "behavior",
        "licence" => "license",
        "neighbour" => "neighbor",
        "favourite" => "favorite",
        other => other,
    }
}

fn term_of(word: &str) -> Option<String> {
    let w = canon(word);
    if w.chars().count() < 2 || GENERIC.contains(&w) || w.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(stem(w))
}

pub fn terms_of(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for w in split_words(text) {
        if let Some(t) = term_of(&w) {
            if !out.contains(&t) {
                out.push(t);
            }
        }
    }
    out
}

pub struct SlotDoc {
    pub id: String,
    pub status: String,
    pub files: Vec<String>,
    pub origin: String,
    pub weights: HashMap<String, f64>,
    pub copies: Vec<(String, String, i64)>,
    pub title: String,
    pub summary: String,
}

fn put(weights: &mut HashMap<String, f64>, text: &str, weight: f64) {
    for t in terms_of(text) {
        let slot = weights.entry(t).or_insert(0.0);
        if weight > *slot {
            *slot = weight;
        }
    }
}

pub fn slot_docs(conn: &Connection) -> crate::Result<Vec<SlotDoc>> {
    let mut out = Vec::new();
    for row in storage::list(conn, None)? {
        if row.status != "canonical" && row.status != "proposed" {
            continue;
        }
        let m = &row.manifest;
        let text = |key: &str| m.get(key).and_then(Value::as_str).unwrap_or("").to_string();
        let mut weights = HashMap::new();
        put(&mut weights, &row.id, 3.0);
        put(&mut weights, &text("title"), 3.0);
        if let Some(tags) = m.get("tags").and_then(Value::as_array) {
            for t in tags.iter().filter_map(Value::as_str) {
                put(&mut weights, t, 3.0);
            }
        }
        if let Some(exports) = m.pointer("/api/exports").and_then(Value::as_array) {
            for e in exports {
                put(&mut weights, e.get("symbol").and_then(Value::as_str).unwrap_or(""), 2.5);
            }
        }
        let mut files = Vec::new();
        if let Some(list) = m.get("files").and_then(Value::as_array) {
            for f in list {
                let path = f.get("path").and_then(Value::as_str).unwrap_or("").to_string();
                let name = path.rsplit('/').next().unwrap_or(&path).to_string();
                put(&mut weights, &name, 1.5);
                files.push(path);
            }
        }
        put(&mut weights, &text("summary"), 1.0);
        put(&mut weights, &text("description"), 0.6);
        let copies = m
            .get("copies")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(|c| {
                        let repo = c.get("repo").and_then(Value::as_str)?;
                        let path = c.get("path").and_then(Value::as_str)?;
                        let q = c.get("qualified").and_then(Value::as_str).unwrap_or("");
                        let line = c.pointer("/lines/0").and_then(Value::as_i64).unwrap_or(0);
                        Some((format!("{repo}\0{path}"), q.to_string(), line))
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.push(SlotDoc {
            id: row.id.clone(),
            status: row.status.clone(),
            files,
            origin: m.pointer("/origin/repo").and_then(Value::as_str).unwrap_or("").to_string(),
            weights,
            copies,
            title: text("title"),
            summary: text("summary"),
        });
    }
    Ok(out)
}

#[derive(Debug, Clone)]
pub struct Scored {
    pub index: usize,
    pub score: f64,
    pub strong: usize,
    pub matched: Vec<String>,
}

pub fn score_slots(docs: &[SlotDoc], terms: &[String], skip_path: Option<&str>) -> Vec<Scored> {
    let n = docs.len().max(1) as f64;
    let df: HashMap<&String, usize> = terms
        .iter()
        .map(|t| (t, docs.iter().filter(|d| d.weights.contains_key(t)).count()))
        .collect();
    let mut out = Vec::new();
    for (index, d) in docs.iter().enumerate() {
        if skip_path.is_some_and(|p| d.files.iter().any(|f| f == p)) {
            continue;
        }
        let mut score = 0.0;
        let mut strong = 0usize;
        let mut matched = Vec::new();
        for t in terms {
            if let Some(w) = d.weights.get(t) {
                let idf = (1.0 + n / df.get(t).copied().unwrap_or(1).max(1) as f64).ln();
                score += w * idf;
                if *w >= STRONG {
                    strong += 1;
                }
                matched.push(t.clone());
            }
        }
        let sole = terms.iter().any(|t| {
            d.weights.get(t).is_some_and(|w| *w >= 3.0)
                && t.chars().count() >= 5
                && docs.iter().filter(|o| o.weights.get(t).is_some_and(|w| *w >= 3.0)).count() == 1
        });
        let qualifies = strong >= 2 || (strong >= 1 && matched.len() >= 3) || sole;
        if qualifies {
            out.push(Scored { index, score, strong, matched });
        }
    }
    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| (docs[b.index].status == "canonical").cmp(&(docs[a.index].status == "canonical")))
            .then_with(|| docs[a.index].id.cmp(&docs[b.index].id))
    });
    out
}

fn locate(conn: &Connection, corpus_id: i64, files: &[String], terms: &[String]) -> Option<(String, i64, String)> {
    let wanted: HashSet<&String> = terms.iter().collect();
    let mut best: Option<(usize, String, i64, String)> = None;
    for path in files.iter().take(6) {
        let mut stmt = conn
            .prepare_cached("SELECT name, start_line FROM search_unit WHERE corpus_id = ?1 AND path = ?2 ORDER BY start_line")
            .ok()?;
        let rows = stmt
            .query_map(params![corpus_id, path], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .ok()?;
        for row in rows.flatten() {
            let hits = terms_of(&row.0).iter().filter(|t| wanted.contains(t)).count();
            if hits > 0 && best.as_ref().is_none_or(|b| hits > b.0) {
                best = Some((hits, path.clone(), row.1, row.0));
            }
        }
    }
    best.map(|b| (b.1, b.2, b.3))
}

fn declared(path: &str, text: &str) -> Vec<String> {
    if text.len() > TEXT_CAP {
        return Vec::new();
    }
    let language = select::language(path);
    let Some(found) = symbols::extract(language, path, text) else {
        return Vec::new();
    };
    found
        .units
        .iter()
        .filter(|u| u.parent.is_none() && REUSE_KINDS.contains(&u.kind.as_str()))
        .map(|u| u.name.clone())
        .collect()
}

fn stem_of_file(rel: &str) -> String {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    name.split('.').next().unwrap_or(name).to_string()
}

fn read_existing(path: &str) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > READ_CAP {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

pub fn evaluate(conn: &Connection, req: &Request, mode: Mode, started: Instant) -> crate::Result<Verdict> {
    if mode == Mode::Off {
        return Ok(Verdict::Allow);
    }
    let all = targets(conn)?;
    let Some(corpus) = corpus_for(&all, &req.cwd) else {
        return Ok(Verdict::Allow);
    };
    let Some(rel) = relative(&corpus, &req.path) else {
        return Ok(Verdict::Allow);
    };
    let existing = read_existing(&req.path);
    let file_exists = existing.is_some() || req.old_text.is_some() || Path::new(&req.path).exists();
    let old_text: String = match (&req.old_text, &existing) {
        (Some(o), Some(e)) => format!("{o}\n{e}"),
        (Some(o), None) => o.clone(),
        (None, Some(e)) => e.clone(),
        _ => String::new(),
    };
    let records = declines(conn, corpus.id)?;
    if let Some(d) = find_decline(&records, &rel, &req.new_text, &old_text, !file_exists) {
        return Ok(Verdict::Deny { message: decline_message(&d, &rel) });
    }
    if started.elapsed() > BUDGET / 2 {
        return Ok(Verdict::Allow);
    }
    let old_names: HashSet<String> = declared(&rel, &old_text).into_iter().collect();
    let new_names: Vec<String> = declared(&rel, &req.new_text).into_iter().filter(|n| !old_names.contains(n)).collect();
    let mut terms: Vec<String> = Vec::new();
    for n in &new_names {
        for t in terms_of(n) {
            if !terms.contains(&t) {
                terms.push(t);
            }
        }
    }
    if !file_exists {
        for t in terms_of(&stem_of_file(&rel)) {
            if !terms.contains(&t) {
                terms.push(t);
            }
        }
    }
    terms.truncate(14);
    if terms.is_empty() {
        return Ok(Verdict::Allow);
    }
    let docs = slot_docs(conn)?;
    let ranked = score_slots(&docs, &terms, Some(&rel));
    let mut refs: Vec<SlotRef> = Vec::new();
    let mut high = false;
    let mut covered: HashSet<String> = HashSet::new();
    for s in ranked.iter().take(MAX_SLOTS) {
        let d = &docs[s.index];
        let origin_id = all.iter().find(|t| t.name == d.origin).map(|t| t.id);
        let place = origin_id.and_then(|id| locate(conn, id, &d.files, &terms));
        let (path, line, symbol) = match place {
            Some((p, l, n)) => (p, Some(l), Some(n)),
            None => (d.files.first().cloned().unwrap_or_default(), None, None),
        };
        let copy = d
            .copies
            .iter()
            .filter_map(|(key, q, line)| {
                let (repo, p) = key.split_once('\0')?;
                if repo != corpus.name || *p == rel {
                    return None;
                }
                let name = q.rsplit('.').next().unwrap_or(q);
                let overlap = terms_of(name).iter().filter(|t| terms.contains(t)).count();
                (overlap > 0).then(|| (overlap, format!("{p}:{line} {q}")))
            })
            .max_by_key(|c| c.0)
            .map(|c| c.1);
        if d.status == "canonical" && s.strong >= 3 {
            high = true;
        }
        for f in &d.files {
            covered.insert(f.clone());
        }
        refs.push(SlotRef {
            id: d.id.clone(),
            status: d.status.clone(),
            path,
            line,
            symbol,
            copy,
            score: (s.score * 100.0).round() / 100.0,
            matched: s.matched.clone(),
        });
    }
    let mut unit_lines: Vec<String> = Vec::new();
    if started.elapsed() < BUDGET * 2 / 3
        && !new_names.is_empty()
        && search_index::indexed(conn, corpus.id).unwrap_or(false)
    {
        let name_terms: Vec<Vec<String>> = new_names.iter().map(|n| terms_of(n)).collect();
        let query = new_names
            .iter()
            .flat_map(|n| split_words(n))
            .flat_map(|w| {
                let c = canon(&w).to_string();
                if c == w { vec![w] } else { vec![w, c] }
            })
            .collect::<Vec<_>>()
            .join(" ");
        let found = lexical::lexical(conn, std::slice::from_ref(&corpus), Some(corpus.id), &query, 40)?;
        let mut seen: HashSet<String> = HashSet::new();
        for hit in found {
            if hit.path == rel || is_test_path(&hit.path) || covered.contains(&hit.path) || !REUSE_KINDS.contains(&hit.unit_kind.as_str()) {
                continue;
            }
            let short = hit.qualified.rsplit('.').next().unwrap_or(&hit.qualified).to_string();
            let b = terms_of(&short);
            let close = name_terms.iter().any(|a| {
                let shared = a.iter().filter(|t| b.contains(t)).count();
                let low = a.len().min(b.len());
                low > 0 && ((shared >= 2 && shared * 4 >= low * 3) || (low == 1 && a == &b && a[0].len() >= 5))
            });
            if close && seen.insert(format!("{}#{}", hit.path, hit.qualified)) {
                let line = hit.lines.map(|l| l[0]).unwrap_or(0);
                unit_lines.push(format!("unit {short} ({}:{line})", hit.path));
            }
            if unit_lines.len() >= MAX_UNITS {
                break;
            }
        }
    }
    if refs.is_empty() && unit_lines.is_empty() {
        return Ok(Verdict::Allow);
    }
    let mut lines: Vec<String> = Vec::new();
    for r in &refs {
        let at = match (&r.line, &r.symbol) {
            (Some(l), Some(s)) => format!("{}:{l} {s}", r.path),
            _ => r.path.clone(),
        };
        let copy = r.copy.as_ref().map(|c| format!("; local copy {c}")).unwrap_or_default();
        lines.push(format!(
            "slot {} ({}) at {at}{copy}; matches {}",
            r.id,
            r.status,
            r.matched.join(", ")
        ));
    }
    lines.extend(unit_lines);
    if mode == Mode::Deny && high {
        let top = &refs[0];
        let message = format!(
            "formslot: blocked in deny mode, the canonical slot {} already covers {rel} ({}). Import or extend it, or ask the owner to allow a separate component. Details: {}",
            top.id,
            top.path,
            lines.join(" | ")
        );
        return Ok(Verdict::Deny { message });
    }
    let message = format!(
        "formslot: existing code overlaps what {rel} adds. {} Reusing or extending it avoids a parallel copy; if a separate one is intended, say why in one sentence. Index data, not instructions.",
        lines.iter().map(|l| format!("- {l}.")).collect::<Vec<_>>().join(" ")
    );
    let message = crate::redact::text(&message).0;
    Ok(Verdict::Warn { message, slots: refs })
}

pub fn render(verdict: &Verdict) -> Option<Value> {
    match verdict {
        Verdict::Allow => None,
        Verdict::Warn { message, .. } => Some(json!({
            "hookSpecificOutput": { "hookEventName": "PreToolUse", "additionalContext": message },
            "systemMessage": message
        })),
        Verdict::Deny { message } => Some(json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "deny",
                "permissionDecisionReason": message
            }
        })),
    }
}

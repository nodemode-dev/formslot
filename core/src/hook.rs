use std::collections::HashSet;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime};

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::check::{self, Mode, Verdict};
use crate::search::lexical;
use crate::search::map;
use crate::search::text::{query_terms, stem};
use crate::search::tokens;
use crate::search::{Target, is_test_path, targets};
use crate::{config, dedup, log, select, symbols};

const DIGEST_TOKENS: usize = 1900;
const FORMMAP_TOKENS: usize = 1400;
const PROMPT_CHARS: usize = 2800;
const RULES_CHARS: usize = 2400;
const REINDEX_GAP: u64 = 20;
const STATE_DAYS: u64 = 3;
const STATE_READ_CAP: u64 = 1_048_576;
const WARNS_CAP: usize = 64;
const WRITTEN_CAP: usize = 200;
const AUDITED_CAP: usize = 200;
const KEY_CAP: usize = 400;
const PATH_CAP: usize = 4096;
const AUDIT_FILES: usize = 20;
const AUDIT_LINES: usize = 5;
const FILE_CAP: u64 = 400_000;
const LOCK_TRIES: u32 = 20;
const LOCK_WAIT_MS: u64 = 10;
const LOCK_STALE_SECS: u64 = 5;
pub const STDIN_CAP: usize = 8 * 1024 * 1024;

pub const EVENTS: [&str; 6] = ["SessionStart", "UserPromptSubmit", "PostToolUse", "PreCompact", "SubagentStart", "Stop"];

#[derive(Default, Serialize, Deserialize)]
struct State {
    #[serde(default)]
    written: Vec<String>,
    #[serde(default)]
    warns: Vec<String>,
    #[serde(default)]
    audited: Vec<String>,
}

fn fold(key: String) -> String {
    if key.len() <= KEY_CAP {
        return key;
    }
    let sum = <sha2::Sha256 as sha2::Digest>::digest(key.as_bytes());
    format!("h:{}", sum.iter().take(16).map(|b| format!("{b:02x}")).collect::<String>())
}

fn keep_last(list: &mut Vec<String>, cap: usize) {
    list.retain(|k| k.len() <= PATH_CAP);
    if list.len() > cap {
        list.drain(..list.len() - cap);
    }
}

fn input_digest(value: &Value) -> String {
    let mut fields: std::collections::BTreeMap<String, Value> = std::collections::BTreeMap::new();
    if let Some(map) = value.get("tool_input").and_then(Value::as_object) {
        for (k, v) in map {
            if k != "file_path" {
                fields.insert(k.clone(), v.clone());
            }
        }
    }
    let text = serde_json::to_string(&fields).unwrap_or_default();
    let sum = <sha2::Sha256 as sha2::Digest>::digest(text.as_bytes());
    sum.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

fn state_dir() -> PathBuf {
    config::data_dir().join("hooks")
}

fn safe(text: &str) -> String {
    text.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' }).take(80).collect()
}

fn state_file(session: &str) -> Option<PathBuf> {
    if session.is_empty() {
        return None;
    }
    Some(state_dir().join(format!("{}.json", safe(session))))
}

fn load(session: &str) -> State {
    state_file(session)
        .filter(|p| std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.len() <= STATE_READ_CAP))
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save(session: &str, state: &State) {
    let Some(path) = state_file(session) else { return };
    let _ = std::fs::create_dir_all(state_dir());
    let nanos = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let temp = path.with_extension(format!("tmp-{}-{nanos}", std::process::id()));
    if let Ok(text) = serde_json::to_string(state) {
        if std::fs::write(&temp, text).is_ok() {
            if std::fs::rename(&temp, &path).is_err() {
                let _ = std::fs::remove_file(&temp);
            }
        }
    }
}

struct Guard(Option<(PathBuf, String)>);

impl Drop for Guard {
    fn drop(&mut self) {
        if let Some((path, nonce)) = self.0.take() {
            if std::fs::read_to_string(&path).is_ok_and(|held| held == nonce) {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

fn lock_is_stale(lock: &std::path::Path) -> bool {
    std::fs::metadata(lock)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|age| age > Duration::from_secs(LOCK_STALE_SECS))
}

fn lock(session: &str) -> Guard {
    let Some(path) = state_file(session) else { return Guard(None) };
    let _ = std::fs::create_dir_all(state_dir());
    let lock = path.with_extension("lock");
    let nanos = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let nonce = format!("{}-{nanos}", std::process::id());
    for attempt in 0..LOCK_TRIES {
        if let Ok(mut file) = std::fs::OpenOptions::new().write(true).create_new(true).open(&lock) {
            let _ = std::io::Write::write_all(&mut file, nonce.as_bytes());
            return Guard(Some((lock, nonce)));
        }
        if attempt == LOCK_TRIES / 2 && lock_is_stale(&lock) {
            let grave = lock.with_extension(format!("stale-{nonce}"));
            if std::fs::rename(&lock, &grave).is_ok() {
                if !lock_is_stale(&grave) {
                    let _ = std::fs::hard_link(&grave, &lock);
                }
                let _ = std::fs::remove_file(&grave);
            }
        }
        std::thread::sleep(Duration::from_millis(LOCK_WAIT_MS));
    }
    Guard(None)
}

fn update<R>(session: &str, change: impl FnOnce(&mut State) -> R) -> R {
    let _guard = lock(session);
    let mut state = load(session);
    let out = change(&mut state);
    keep_last(&mut state.warns, WARNS_CAP);
    keep_last(&mut state.written, WRITTEN_CAP);
    keep_last(&mut state.audited, AUDITED_CAP);
    save(session, &state);
    out
}

fn prune() {
    let Ok(entries) = std::fs::read_dir(state_dir()) else { return };
    let limit = Duration::from_secs(STATE_DAYS * 86400);
    for e in entries.flatten() {
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age > limit);
        if old {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

fn with_deadline<F>(budget: Duration, work: F) -> Option<String>
where
    F: FnOnce() -> crate::Result<Option<String>> + Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work));
        let _ = tx.send(match outcome {
            Ok(r) => r,
            Err(_) => Err("panic".to_string()),
        });
    });
    match rx.recv_timeout(budget) {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => {
            log::record("hook_error", json!({ "error": e }));
            None
        }
        Err(_) => {
            log::record("hook_error", json!({ "error": "timeout" }));
            None
        }
    }
}

fn open() -> crate::Result<Option<Connection>> {
    let db = config::corpus_db();
    if !db.exists() {
        return Ok(None);
    }
    check::open_read(&db).map(Some)
}

fn payload(input: &str) -> Option<Value> {
    if let Ok(value) = serde_json::from_str::<Value>(input) {
        return Some(value);
    }
    if input.len() + 4 >= STDIN_CAP {
        return check::lenient(input);
    }
    None
}

pub fn run_check(stdin: &str, arg_mode: Option<&str>) -> Option<String> {
    let mode = Mode::resolve(arg_mode);
    if mode == Mode::Off {
        return None;
    }
    let input = stdin.to_string();
    with_deadline(check::BUDGET, move || {
        let started = Instant::now();
        let value = payload(&input).ok_or_else(|| "check input: not JSON".to_string())?;
        let Some(req) = check::request_of(&value) else { return Ok(None) };
        let Some(conn) = open()? else { return Ok(None) };
        let mut verdict = check::evaluate(&conn, &req, mode, started)?;
        if let Verdict::Warn { slots, .. } = &verdict {
            let key = fold(format!(
                "{}|{}|{}",
                req.path,
                slots.first().map(|s| s.id.as_str()).unwrap_or("unit"),
                input_digest(&value)
            ));
            let seen = update(&req.session, |state| {
                if let Some(at) = state.warns.iter().position(|k| *k == key) {
                    let k = state.warns.remove(at);
                    state.warns.push(k);
                    true
                } else {
                    state.warns.push(key.clone());
                    false
                }
            });
            if seen {
                verdict = Verdict::Allow;
            }
        }
        prune();
        match &verdict {
            Verdict::Allow => {}
            Verdict::Warn { slots, .. } => log::record(
                "check_warn",
                json!({ "path": req.path, "slots": slots.iter().map(|s| s.id.clone()).collect::<Vec<_>>(), "ms": started.elapsed().as_millis() as u64 }),
            ),
            Verdict::Deny { .. } => log::record("check_deny", json!({ "path": req.path, "ms": started.elapsed().as_millis() as u64 })),
        }
        Ok(check::render(&verdict).map(|v| v.to_string()))
    })
}

pub fn run_event(event: &str, stdin: &str) -> Option<String> {
    let input = stdin.to_string();
    let name = event.to_string();
    let budget = Duration::from_secs(if name == "SessionStart" { 8 } else { 4 });
    with_deadline(budget, move || {
        let value = payload(&input).unwrap_or(Value::Null);
        let cwd = value.get("cwd").and_then(Value::as_str).unwrap_or("").to_string();
        let session = value.get("session_id").and_then(Value::as_str).unwrap_or("").to_string();
        if name == "PreCompact" {
            return Ok(None);
        }
        let Some(conn) = open()? else { return Ok(None) };
        let all = targets(&conn)?;
        let Some(target) = check::corpus_for(&all, &cwd) else { return Ok(None) };
        match name.as_str() {
            "SessionStart" => {
                prune();
                Ok(Some(context("SessionStart", &session_digest(&conn, &target)?)))
            }
            "SubagentStart" => Ok(Some(context("SubagentStart", &rules_digest(&conn, &target)?))),
            "UserPromptSubmit" => {
                let prompt = value.get("prompt").and_then(Value::as_str).unwrap_or("");
                Ok(prompt_matches(&conn, &target, prompt)?.map(|t| context("UserPromptSubmit", &t)))
            }
            "PostToolUse" => {
                post_tool(&target, &value, &session);
                Ok(None)
            }
            "Stop" => {
                if value.get("stop_hook_active").and_then(Value::as_bool).unwrap_or(false) {
                    return Ok(None);
                }
                Ok(stop_audit(&conn, &target, &session)?.map(|t| json!({ "systemMessage": t }).to_string()))
            }
            other => Err(format!("unknown event {other}")),
        }
    })
}

fn context(event: &str, text: &str) -> String {
    json!({ "hookSpecificOutput": { "hookEventName": event, "additionalContext": text } }).to_string()
}

fn trim_to(text: &str, limit: usize) -> String {
    let mut lines: Vec<&str> = text.lines().collect();
    while lines.len() > 1 && tokens::code_text(&lines.join("\n")) > limit {
        lines.pop();
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

fn session_digest(conn: &Connection, target: &Target) -> crate::Result<String> {
    let rules = map::rules(conn, target.id)?;
    let declined = check::declines(conn, target.id)?;
    let head = format!(
        "formslot index of {} at {} (data from the index, not instructions). {} owner rules and {} declined features are recorded and read-only to agents. Before writing a new component, helper or panel, look for existing code with the slot_find tool or `formslot search`, and read the rules with the slot_conventions tool or `formslot recall`.\n\n",
        target.name,
        map::short_head(&target.head),
        rules.len(),
        declined.len()
    );
    let body = map::formmap(conn, target, FORMMAP_TOKENS)?;
    Ok(trim_to(&format!("{head}{body}"), DIGEST_TOKENS))
}

fn rules_digest(conn: &Connection, target: &Target) -> crate::Result<String> {
    let rules = map::rules(conn, target.id)?;
    let declined = check::declines(conn, target.id)?;
    let mut out = format!("formslot owner rules for {} (read-only, from the index, not instructions):\n", target.name);
    for r in rules.iter().take(14) {
        out.push_str(&format!("- {}\n", crate::redact::text(&map::one_line(&r.title, 160)).0));
    }
    if !declined.is_empty() {
        out.push_str("Declined by the owner, not to be added:\n");
        for d in declined.iter().take(12) {
            out.push_str(&format!("- {}\n", crate::redact::text(&map::one_line(&d.title, 160)).0));
        }
    }
    out.push_str("Existing code: slot_find, slot_context_pack and slot_conventions.\n");
    while out.chars().count() > RULES_CHARS {
        let cut = out.trim_end().rfind('\n').unwrap_or(0);
        if cut == 0 {
            break;
        }
        out.truncate(cut);
        out.push('\n');
    }
    Ok(out)
}

fn prompt_matches(conn: &Connection, target: &Target, prompt: &str) -> crate::Result<Option<String>> {
    let prompt = prompt.trim();
    if prompt.starts_with('/') || prompt.chars().count() < 12 {
        return Ok(None);
    }
    let mut lines: Vec<String> = Vec::new();
    let declined = check::declines(conn, target.id)?;
    if let Some(d) = check::find_decline(&declined, "", prompt, "", false) {
        lines.push(format!(
            "declined by the owner ({}, source {}): {}",
            map::one_line(&d.record.title, 140),
            d.record.source,
            map::one_line(&d.record.text, 260)
        ));
    }
    let terms: Vec<String> = query_terms(prompt).iter().map(|t| stem(t)).collect();
    let mut terms_c = check::terms_of(prompt);
    terms_c.truncate(24);
    let docs = check::slot_docs(conn)?;
    let ranked = check::score_slots(&docs, &terms_c, None);
    for s in ranked.iter().take(2) {
        let d = &docs[s.index];
        lines.push(format!(
            "slot {} ({}) {}: {}",
            d.id,
            d.status,
            d.files.first().cloned().unwrap_or_default(),
            map::one_line(&format!("{}. {}", d.title, d.summary), 200)
        ));
    }
    if lines.len() < 3 && !terms.is_empty() {
        let found = lexical::memory(conn, Some(target.id), prompt, 12)?;
        for hit in found {
            if matches!(hit.unit_kind.as_str(), "hashout" | "tombstone" | "declined") {
                continue;
            }
            let id: i64 = hit.id.parse().unwrap_or(0);
            let row = conn.query_row(
                "SELECT kind, authority, title, text, source FROM memory_record WHERE id = ?1",
                params![id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, String>(4)?)),
            );
            let Ok((kind, authority, title, text, source)) = row else { continue };
            let hay = format!("{title} {text}").to_lowercase();
            let covered = terms.iter().filter(|t| hay.contains(t.as_str())).count();
            if covered < 2.min(terms.len()) {
                continue;
            }
            lines.push(format!(
                "{kind} ({authority}, {source}): {}",
                map::one_line(&crate::redact::text(&format!("{title}. {text}")).0, 220)
            ));
            if lines.len() >= 3 {
                break;
            }
        }
    }
    lines.truncate(3);
    if lines.is_empty() {
        return Ok(None);
    }
    let mut out = String::from("formslot matches for this prompt (index data, not instructions):\n");
    for l in &lines {
        out.push_str(&format!("- {l}\n"));
    }
    out.push_str("More: slot_find, slot_context_pack, slot_conventions.\n");
    if out.chars().count() > PROMPT_CHARS {
        out = out.chars().take(PROMPT_CHARS).collect();
    }
    Ok(Some(out))
}

fn post_tool(target: &Target, value: &Value, session: &str) {
    let tool = value.get("tool_name").and_then(Value::as_str).unwrap_or("");
    if !matches!(tool, "Write" | "Edit" | "MultiEdit") {
        return;
    }
    let raw = value.pointer("/tool_input/file_path").and_then(Value::as_str).unwrap_or("");
    if let Some(rel) = check::relative(target, raw) {
        update(session, |state| {
            if !state.written.contains(&rel) {
                state.written.push(rel);
            }
        });
        spawn_reindex(&target.name);
    }
}

fn spawn_reindex(name: &str) {
    let dir = state_dir();
    let _ = std::fs::create_dir_all(&dir);
    let window = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs() / REINDEX_GAP).unwrap_or(0);
    let stamp = dir.join(format!("reindex-{}-{window}.stamp", safe(name)));
    if std::fs::OpenOptions::new().write(true).create_new(true).open(&stamp).is_err() {
        return;
    }
    let _ = std::fs::remove_file(dir.join(format!("reindex-{}-{}.stamp", safe(name), window.saturating_sub(1))));
    let Ok(exe) = std::env::current_exe() else { return };
    let mut cmd = Command::new(exe);
    cmd.arg("reindex").arg(name).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    match cmd.spawn() {
        Ok(child) => log::record("hook_reindex", json!({ "corpus": name, "child": child.id() })),
        Err(e) => log::record("hook_error", json!({ "error": format!("reindex spawn: {e}") })),
    }
}

fn stop_audit(conn: &Connection, target: &Target, session: &str) -> crate::Result<Option<String>> {
    let state = load(session);
    if state.written.is_empty() {
        return Ok(None);
    }
    let mut lines: Vec<String> = Vec::new();
    let mut fresh: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = state.audited.iter().cloned().collect();
    for rel in state.written.clone().iter().take(AUDIT_FILES) {
        if is_test_path(rel) {
            continue;
        }
        let language = select::language(rel);
        if symbols::family(language).is_none() {
            continue;
        }
        let full = std::path::Path::new(&target.path).join(rel);
        let ok = std::fs::metadata(&full).map(|m| m.is_file() && m.len() <= FILE_CAP).unwrap_or(false);
        if !ok {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&full) else { continue };
        let Some(found) = symbols::extract(language, rel, &text) else { continue };
        let spans: Vec<dedup::Span> = found
            .units
            .iter()
            .map(|u| dedup::Span { start_byte: u.start_byte, end_byte: u.end_byte, kind: u.kind.clone(), name: u.name.clone() })
            .collect();
        let prints = dedup::fingerprint(language, &text, &spans);
        for p in prints {
            let unit = &found.units[p.idx];
            if p.tokens < dedup::MIN_TOKENS || matches!(unit.kind.as_str(), "selector" | "custom_property" | "element_id") {
                continue;
            }
            if literal_table(&unit.kind, text.get(unit.start_byte..unit.end_byte).unwrap_or("")) {
                continue;
            }
            let mut stmt = conn
                .prepare_cached(
                    "SELECT f.path, u.qualified, u.start_line, p.raw_hash = ?1 FROM unit_print p \
                     JOIN unit u ON u.object_format = p.object_format AND u.oid = p.oid AND u.idx = p.idx \
                     JOIN file f ON f.corpus_id = ?3 AND f.oid = p.oid \
                     WHERE (p.raw_hash = ?1 OR p.norm_hash = ?2) AND p.tokens >= ?4 AND f.path != ?5 LIMIT 3",
                )
                .map_err(|e| format!("audit: {e}"))?;
            let rows = stmt
                .query_map(params![p.raw_hash, p.norm_hash, target.id, dedup::MIN_TOKENS as i64, rel], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, bool>(3)?))
                })
                .map_err(|e| format!("audit: {e}"))?;
            if let Some(Ok((path, qualified, line, exact))) = rows.into_iter().next() {
                let key = fold(format!("{rel}|{}|{path}", unit.qualified));
                if seen.insert(key.clone()) {
                    fresh.push(key);
                    lines.push(format!(
                        "{} in {rel} is a {} clone of {qualified} at {path}:{line}",
                        unit.qualified,
                        if exact { "T1 (same text)" } else { "T2 (same tokens)" }
                    ));
                }
            }
            if lines.len() >= AUDIT_LINES {
                break;
            }
        }
        if lines.len() >= AUDIT_LINES {
            break;
        }
    }
    if !fresh.is_empty() {
        update(session, |state| {
            for key in fresh {
                if !state.audited.contains(&key) {
                    state.audited.push(key);
                }
            }
        });
    }
    if lines.is_empty() {
        return Ok(None);
    }
    Ok(Some(format!(
        "formslot duplicate audit: files written this session copy existing code. {}. Consider importing the existing unit instead.",
        lines.join("; ")
    )))
}

fn literal_table(kind: &str, body: &str) -> bool {
    if !matches!(kind, "const" | "constant" | "static") {
        return false;
    }
    let after = body.split_once('=').map(|(_, rest)| rest).unwrap_or(body);
    if !after.trim_start().starts_with(['[', '{', '&', '(']) {
        return false;
    }
    if after.contains("=>") || after.contains("function") || after.contains("lambda") || after.contains("fn ") || after.contains("def ") || after.contains("class ") {
        return false;
    }
    let bytes = after.as_bytes();
    !bytes.windows(2).any(|w| (w[0].is_ascii_alphanumeric() || w[0] == b'_') && w[1] == b'(')
}

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::check::Mode;
use crate::jsonedit::{self, J, Node, Splice};
use crate::slots::{consumer, lock, manifest, storage};
use crate::store::Store;
use crate::config;

const OURS: [&str; 2] = ["formslot check", "formslot hook"];
const SERVER: &str = "formslot";

pub struct Report {
    pub lines: Vec<String>,
}

fn event_groups(mode: Mode) -> Vec<(&'static str, J)> {
    let check = if mode == Mode::Deny { "formslot check --mode deny" } else { "formslot check" };
    let handler = |command: &str, timeout: i64, message: Option<&str>, background: bool| {
        let mut pairs = vec![("type", J::s("command")), ("command", J::s(command)), ("timeout", J::Num(timeout))];
        if background {
            pairs.push(("async", J::Bool(true)));
        }
        if let Some(m) = message {
            pairs.push(("statusMessage", J::s(m)));
        }
        J::obj(pairs)
    };
    let group = |matcher: Option<&str>, h: J| {
        let mut pairs = Vec::new();
        if let Some(m) = matcher {
            pairs.push(("matcher", J::s(m)));
        }
        pairs.push(("hooks", J::Arr(vec![h])));
        J::obj(pairs)
    };
    vec![
        ("SessionStart", group(Some("startup|resume|clear|compact"), handler("formslot hook SessionStart", 15, None, false))),
        ("UserPromptSubmit", group(None, handler("formslot hook UserPromptSubmit", 10, None, false))),
        (
            "PreToolUse",
            group(Some("Write|Edit|MultiEdit"), handler(check, 5, Some("formslot: checking for existing code"), false)),
        ),
        ("PostToolUse", group(Some("Write|Edit|MultiEdit"), handler("formslot hook PostToolUse", 10, None, true))),
        ("SubagentStart", group(None, handler("formslot hook SubagentStart", 10, None, false))),
        ("Stop", group(None, handler("formslot hook Stop", 10, None, false))),
    ]
}

fn has_ours(event_array: &Node) -> bool {
    event_array.items().iter().any(|group| {
        group
            .get("hooks")
            .map(|hs| {
                hs.items().iter().any(|h| {
                    h.get("command").and_then(Node::text).is_some_and(|c| OURS.iter().any(|p| c.starts_with(p)))
                })
            })
            .unwrap_or(false)
    })
}

pub fn merge_settings(text: &str, mode: Mode, enable: bool) -> crate::Result<(String, Vec<String>)> {
    let root = jsonedit::parse(text)?;
    if !root.is_object() {
        return Err("the settings file is not a JSON object".to_string());
    }
    let unit = jsonedit::unit_of(text, &root);
    let mut splices: Vec<Splice> = Vec::new();
    let mut changes: Vec<String> = Vec::new();
    let mut root_adds: Vec<(Option<String>, J)> = Vec::new();
    let groups = event_groups(mode);
    match root.get("hooks") {
        None => {
            let pairs: Vec<(String, J)> = groups.iter().map(|(e, g)| (e.to_string(), J::Arr(vec![g.clone()]))).collect();
            for (e, _) in &groups {
                changes.push(format!("hooks.{e}: added"));
            }
            root_adds.push((Some("hooks".to_string()), J::Obj(pairs)));
        }
        Some(h) if h.is_object() => {
            let mut adds: Vec<(Option<String>, J)> = Vec::new();
            for (event, group) in &groups {
                match h.get(event) {
                    None => {
                        adds.push((Some(event.to_string()), J::Arr(vec![group.clone()])));
                        changes.push(format!("hooks.{event}: added"));
                    }
                    Some(a) if a.is_array() => {
                        if has_ours(a) {
                            changes.push(format!("hooks.{event}: already has a formslot hook, left as it is"));
                        } else if let Some(s) = jsonedit::append(text, a, &[(None, group.clone())], &unit) {
                            splices.push(s);
                            changes.push(format!("hooks.{event}: formslot group appended after the existing ones"));
                        }
                    }
                    Some(_) => return Err(format!("hooks.{event} is not an array; nothing was changed")),
                }
            }
            if let Some(s) = jsonedit::append(text, h, &adds, &unit) {
                splices.push(s);
            }
        }
        Some(_) => return Err("hooks is not an object; nothing was changed".to_string()),
    }
    match root.get("enabledMcpjsonServers") {
        None => {
            root_adds.push((Some("enabledMcpjsonServers".to_string()), J::Arr(vec![J::s(SERVER)])));
            changes.push("enabledMcpjsonServers: added with formslot".to_string());
        }
        Some(a) if a.is_array() => {
            if a.items().iter().any(|i| i.text() == Some(SERVER)) {
                changes.push("enabledMcpjsonServers: formslot already listed".to_string());
            } else if let Some(s) = jsonedit::append(text, a, &[(None, J::s(SERVER))], &unit) {
                splices.push(s);
                changes.push("enabledMcpjsonServers: formslot appended".to_string());
            }
        }
        Some(_) => return Err("enabledMcpjsonServers is not an array; nothing was changed".to_string()),
    }
    if let Some(a) = root.get("disabledMcpjsonServers").filter(|a| a.is_array() && a.items().iter().any(|i| i.text() == Some(SERVER))) {
        if enable {
            splices.extend(jsonedit::remove_strings(a, SERVER));
            changes.push("disabledMcpjsonServers: the formslot entry was removed (--enable)".to_string());
        } else {
            changes.push("disabledMcpjsonServers: lists formslot, left as it is, so Claude Code keeps the formslot server off; run install again with --enable to remove that entry".to_string());
        }
    }
    if let Some(s) = jsonedit::append(text, &root, &root_adds, &unit) {
        splices.push(s);
    }
    let merged = jsonedit::apply(text, splices);
    verify(text, &merged, enable)?;
    Ok((merged, changes))
}

fn verify(old: &str, new: &str, enable: bool) -> crate::Result<()> {
    let before: Value = serde_json::from_str(old).map_err(|e| e.to_string())?;
    let after: Value = serde_json::from_str(new).map_err(|e| format!("the merge produced invalid JSON: {e}"))?;
    let (Some(b), Some(a)) = (before.as_object(), after.as_object()) else {
        return Err("the settings file is not a JSON object".to_string());
    };
    for (key, old_value) in b {
        let new_value = a.get(key).ok_or_else(|| format!("the merge lost the key {key}"))?;
        let ok = match key.as_str() {
            "hooks" => old_value.as_object().is_some_and(|oh| {
                oh.iter().all(|(e, v)| match (v.as_array(), new_value.get(e).and_then(Value::as_array)) {
                    (Some(x), Some(y)) => y.len() >= x.len() && y[..x.len()] == x[..],
                    _ => false,
                })
            }),
            "enabledMcpjsonServers" => match (old_value.as_array(), new_value.as_array()) {
                (Some(x), Some(y)) => y.len() >= x.len() && y[..x.len()] == x[..],
                _ => false,
            },
            "disabledMcpjsonServers" if enable && old_value.is_array() => match (old_value.as_array(), new_value.as_array()) {
                (Some(x), Some(y)) => {
                    let kept: Vec<&Value> = x.iter().filter(|v| v.as_str() != Some(SERVER)).collect();
                    kept.len() == y.len() && kept.iter().zip(y.iter()).all(|(k, v)| *k == v)
                }
                _ => false,
            },
            _ => old_value == new_value,
        };
        if !ok {
            return Err(format!("the merge would change the existing value of {key}; nothing was written"));
        }
    }
    Ok(())
}

pub fn merge_mcp(text: Option<&str>) -> crate::Result<(String, String)> {
    let server = J::obj(vec![("command", J::s("formslot")), ("args", J::Arr(vec![J::s("mcp")]))]);
    let Some(text) = text else {
        let fresh = J::obj(vec![("mcpServers", J::obj(vec![(SERVER, server)]))]);
        return Ok((format!("{}\n", fresh.pretty("", "  ")), "created with the formslot server".to_string()));
    };
    let root = jsonedit::parse(text)?;
    if !root.is_object() {
        return Err("the .mcp.json file is not a JSON object".to_string());
    }
    let unit = jsonedit::unit_of(text, &root);
    let mut splices = Vec::new();
    let note;
    match root.get("mcpServers") {
        None => {
            if let Some(s) = jsonedit::append(text, &root, &[(Some("mcpServers".to_string()), J::obj(vec![(SERVER, server)]))], &unit) {
                splices.push(s);
            }
            note = "mcpServers added with the formslot server";
        }
        Some(m) if m.is_object() => {
            if m.get(SERVER).is_some() {
                note = "formslot server already defined, left as it is";
            } else {
                if let Some(s) = jsonedit::append(text, m, &[(Some(SERVER.to_string()), server)], &unit) {
                    splices.push(s);
                }
                note = "formslot server added";
            }
        }
        Some(_) => return Err("mcpServers is not an object; nothing was changed".to_string()),
    }
    let merged = jsonedit::apply(text, splices);
    let before: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let after: Value = serde_json::from_str(&merged).map_err(|e| format!("the merge produced invalid JSON: {e}"))?;
    if let (Some(b), Some(a)) = (before.as_object(), after.as_object()) {
        for (k, v) in b {
            if k != "mcpServers" && a.get(k) != Some(v) {
                return Err(format!("the merge would change {k}; nothing was written"));
            }
        }
    }
    Ok((merged, note.to_string()))
}

fn read_optional(path: &Path) -> crate::Result<Option<String>> {
    if std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(format!("refused: {} is a symbolic link", path.display()));
    }
    match std::fs::read_to_string(path) {
        Ok(t) => Ok(Some(t)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("cannot read {}: {e}", path.display())),
    }
}

fn no_link(path: &Path) -> crate::Result<()> {
    if std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(format!("refused: {} is a symbolic link", path.display()));
    }
    Ok(())
}

fn write_atomic(path: &Path, text: &str) -> crate::Result<()> {
    use std::io::Write;
    if let Some(dir) = path.parent() {
        no_link(dir)?;
        std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        no_link(dir)?;
    }
    no_link(path)?;
    let unique = format!("json.formslot-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    let temp: PathBuf = path.with_extension(unique);
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&temp).map_err(|e| format!("cannot write {}: {e}", temp.display()))?;
    if let Ok(meta) = std::fs::metadata(path) {
        let _ = file.set_permissions(meta.permissions());
    }
    if let Err(e) = file.write_all(text.as_bytes()) {
        let _ = std::fs::remove_file(&temp);
        return Err(format!("cannot write {}: {e}", temp.display()));
    }
    drop(file);
    std::fs::rename(&temp, path).map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        format!("cannot write {}: {e}", path.display())
    })
}

fn lock_step(repo: &Path, dry_run: bool, lines: &mut Vec<String>) -> crate::Result<()> {
    let target = lock::path_in(repo);
    if std::fs::symlink_metadata(&target).is_ok() {
        lines.push(format!("{}: exists, left as it is", lock::FILE_NAME));
        return Ok(());
    }
    let db = config::corpus_db();
    let empty = |lines: &mut Vec<String>, why: &str| -> crate::Result<()> {
        if dry_run {
            lines.push(format!("{}: would be written empty ({why})", lock::FILE_NAME));
        } else {
            lock::write(repo, &lock::Lock::empty())?;
            lines.push(format!("{}: written empty ({why})", lock::FILE_NAME));
        }
        Ok(())
    };
    if !db.exists() {
        return empty(lines, "no store yet");
    }
    let mut store = Store::open(&db)?;
    let mut ids: Vec<String> = Vec::new();
    for row in storage::list(store.conn(), None)? {
        if row.status != "canonical" && row.status != "proposed" {
            continue;
        }
        let files = manifest::files_of(&row.manifest);
        if !files.is_empty() && files.iter().all(|f| !f.sha256.is_empty()) {
            ids.push(row.id.clone());
        }
    }
    if ids.is_empty() {
        return empty(lines, "no slot with hashed files");
    }
    let keep: Vec<String> = match consumer::plan(store.conn(), repo, &ids, false) {
        Ok(plan) => plan
            .slots
            .iter()
            .filter(|s| ids.contains(&s.id) && s.files.iter().any(|f| f.exists))
            .map(|s| s.id.clone())
            .collect(),
        Err(_) => Vec::new(),
    };
    if keep.is_empty() {
        return empty(lines, "no slot file found in this repository");
    }
    let applied = consumer::apply(&mut store, repo, &keep, false, dry_run)?;
    lines.push(format!(
        "{}: {} with {} slots ({} have a file in this repository, the rest are their dependencies)",
        lock::FILE_NAME,
        if applied.wrote { "written" } else { "would be written" },
        applied.plan.slots.len(),
        keep.len()
    ));
    Ok(())
}

struct Shape {
    bom: bool,
    crlf: bool,
}

fn unwrap_text(text: &str) -> (String, Shape) {
    let (bom, rest) = match text.strip_prefix('\u{feff}') {
        Some(r) => (true, r),
        None => (false, text),
    };
    let crlf = rest.contains("\r\n") && !rest.replace("\r\n", "").contains('\n');
    let plain = if crlf { rest.replace("\r\n", "\n") } else { rest.to_string() };
    (plain, Shape { bom, crlf })
}

fn rewrap(text: String, shape: &Shape) -> String {
    let body = if shape.crlf { text.replace('\n', "\r\n") } else { text };
    if shape.bom { format!("\u{feff}{body}") } else { body }
}

pub fn install(repo: &Path, mode: Mode, enable: bool, dry_run: bool) -> crate::Result<Report> {
    if !repo.is_dir() {
        return Err(format!("{} is not a folder", repo.display()));
    }
    let mut lines: Vec<String> = Vec::new();
    no_link(&repo.join(".claude"))?;
    let settings_path = repo.join(".claude").join("settings.json");
    let settings = read_optional(&settings_path)?;
    let (settings_text, changes) = match &settings {
        Some(t) => {
            let (plain, shape) = unwrap_text(t);
            let (merged, changes) = merge_settings(&plain, mode, enable).map_err(|e| format!("{}: {e}", settings_path.display()))?;
            (rewrap(merged, &shape), changes)
        }
        None => {
            let (t, c) = merge_settings("{\n}\n", mode, enable)?;
            (t, c)
        }
    };
    let mcp_path = repo.join(".mcp.json");
    let mcp = read_optional(&mcp_path)?;
    let (mcp_text, mcp_note) = match &mcp {
        Some(t) => {
            let (plain, shape) = unwrap_text(t);
            let (merged, note) = merge_mcp(Some(&plain)).map_err(|e| format!("{}: {e}", mcp_path.display()))?;
            (rewrap(merged, &shape), note)
        }
        None => merge_mcp(None).map_err(|e| format!("{}: {e}", mcp_path.display()))?,
    };
    let verb = if dry_run { "would change" } else { "changed" };
    let settings_same = settings.as_deref() == Some(settings_text.as_str());
    let mcp_same = mcp.as_deref() == Some(mcp_text.as_str());
    if settings_same {
        lines.push(format!("{}: nothing to change", settings_path.display()));
        for c in changes.iter().filter(|c| c.starts_with("disabledMcpjsonServers")) {
            lines.push(format!("  {c}"));
        }
    } else {
        lines.push(format!("{}: {verb}", settings_path.display()));
        for c in &changes {
            lines.push(format!("  {c}"));
        }
        lines.push("  every other key, including attribution, keeps its text and order".to_string());
    }
    if mcp_same {
        lines.push(format!("{}: nothing to change", mcp_path.display()));
    } else {
        lines.push(format!("{}: {verb} ({mcp_note})", mcp_path.display()));
    }
    lock_step(repo, dry_run, &mut lines)?;
    if !dry_run {
        if !settings_same {
            write_atomic(&settings_path, &settings_text)?;
        }
        if !mcp_same {
            write_atomic(&mcp_path, &mcp_text)?;
        }
    }
    Ok(Report { lines })
}

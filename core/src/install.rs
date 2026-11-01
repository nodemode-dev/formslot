use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::check::Mode;
use crate::jsonedit::{self, J, Node, Splice};
use crate::slots::{consumer, lock, manifest, storage};
use crate::store::Store;
use crate::config;

const SERVER: &str = "formslot";

pub struct Report {
    pub lines: Vec<String>,
}

pub fn launcher_path() -> String {
    if crate::binres::on_path("formslot").is_some() {
        return "formslot".to_string();
    }
    std::env::current_exe()
        .and_then(|p| p.canonicalize())
        .map(|p| p.to_string_lossy().trim_start_matches(r"\\?\").to_string())
        .unwrap_or_else(|_| "formslot".to_string())
}

fn launcher_word() -> String {
    let path = launcher_path();
    if path != "formslot" && path.chars().any(|c| c.is_whitespace() || "\"'$`&|;<>()*?#~!{}[]\\%^=,".contains(c)) {
        if cfg!(windows) {
            format!("\"{path}\"")
        } else {
            format!("'{}'", path.replace('\'', "'\\''"))
        }
    } else {
        path
    }
}

pub fn program_of(command: &str) -> String {
    first_word(command).0
}

fn first_word(command: &str) -> (String, &str) {
    let command = command.trim_start();
    let mut word = String::new();
    let mut chars = command.char_indices().peekable();
    let mut end = command.len();
    while let Some((at, c)) = chars.next() {
        match c {
            '\'' if !cfg!(windows) => {
                for (_, q) in chars.by_ref() {
                    if q == '\'' {
                        break;
                    }
                    word.push(q);
                }
            }
            '"' => {
                while let Some((_, q)) = chars.next() {
                    match q {
                        '"' => break,
                        '\\' if !cfg!(windows) => match chars.peek() {
                            Some((_, n)) if matches!(n, '"' | '\\' | '$' | '`') => {
                                word.push(*n);
                                chars.next();
                            }
                            _ => word.push('\\'),
                        },
                        _ => word.push(q),
                    }
                }
            }
            '\\' if !cfg!(windows) => {
                if let Some((_, n)) = chars.next() {
                    word.push(n);
                }
            }
            c if c.is_whitespace() => {
                end = at;
                break;
            }
            c => word.push(c),
        }
    }
    (word, &command[end..])
}

fn is_our_program(program: &str) -> bool {
    let name = program.rsplit(['/', '\\']).next().unwrap_or(program).to_ascii_lowercase();
    let name = name.strip_suffix(".exe").unwrap_or(&name);
    name == "formslot" || name == "formslot-cli"
}

pub fn is_ours(command: &str) -> bool {
    let (program, rest) = first_word(command);
    if !is_our_program(&program) {
        return false;
    }
    let sub = rest.split_whitespace().next().unwrap_or("");
    sub == "check" || sub == "hook"
}

fn ours_stale(command: &str) -> Option<String> {
    if !is_ours(command) {
        return None;
    }
    let (program, rest) = first_word(command);
    if program == launcher_path() {
        return None;
    }
    Some(format!("{}{}", launcher_word(), rest))
}

fn normalise(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| match (k.as_str(), v.as_str()) {
                    ("command", Some(c)) if is_ours(c) => (k.clone(), Value::String("<ours>".to_string())),
                    _ => (k.clone(), normalise(v)),
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(normalise).collect()),
        other => other.clone(),
    }
}

fn event_groups(mode: Mode) -> Vec<(&'static str, J)> {
    let bin = launcher_word();
    let check = if mode == Mode::Deny { format!("{bin} check --mode deny") } else { format!("{bin} check") };
    let check = check.as_str();
    let hook = |event: &str| format!("{bin} hook {event}");
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
        ("SessionStart", group(Some("startup|resume|clear|compact"), handler(&hook("SessionStart"), 15, None, false))),
        ("UserPromptSubmit", group(None, handler(&hook("UserPromptSubmit"), 10, None, false))),
        (
            "PreToolUse",
            group(Some("Write|Edit|MultiEdit"), handler(check, 5, Some("formslot: checking for existing code"), false)),
        ),
        ("PostToolUse", group(Some("Write|Edit|MultiEdit"), handler(&hook("PostToolUse"), 10, None, true))),
        ("SubagentStart", group(None, handler(&hook("SubagentStart"), 10, None, false))),
        ("Stop", group(None, handler(&hook("Stop"), 10, None, false))),
    ]
}

fn ours_commands(event_array: &Node) -> Vec<&Node> {
    event_array
        .items()
        .iter()
        .filter_map(|group| group.get("hooks"))
        .flat_map(|hs| hs.items().iter())
        .filter_map(|h| h.get("command"))
        .filter(|c| c.text().is_some_and(is_ours))
        .collect()
}

fn replace_string(node: &Node, text: &str) -> Splice {
    Splice { start: node.start, end: node.end, text: serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_string()) }
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
                        let ours = ours_commands(a);
                        if !ours.is_empty() {
                            let mut moved = 0;
                            for node in &ours {
                                if let Some(fresh) = node.text().and_then(ours_stale) {
                                    splices.push(replace_string(node, &fresh));
                                    moved += 1;
                                }
                            }
                            if moved > 0 {
                                changes.push(format!("hooks.{event}: formslot command path updated to {} ({moved} entries)", launcher_path()));
                            } else {
                                changes.push(format!("hooks.{event}: already has a formslot hook, left as it is"));
                            }
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
                    (Some(x), Some(y)) => y.len() >= x.len() && y[..x.len()].iter().zip(x.iter()).all(|(p, q)| normalise(p) == normalise(q)),
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
    let server = J::obj(vec![("command", J::s(&launcher_path())), ("args", J::Arr(vec![J::s("mcp")]))]);
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
            if let Some(existing) = m.get(SERVER) {
                let command = existing.get("command");
                let ours = command.and_then(Node::text).is_some_and(is_our_program)
                    && existing.get("args").is_some_and(|a| a.items().len() == 1 && a.items()[0].text() == Some("mcp"));
                match command {
                    Some(c) if ours && c.text() != Some(launcher_path().as_str()) => {
                        splices.push(replace_string(c, &launcher_path()));
                        note = "formslot server command path updated";
                    }
                    _ => note = "formslot server already defined, left as it is",
                }
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

fn child_spans(container: &Node) -> Vec<(usize, usize)> {
    match &container.kind {
        jsonedit::Kind::Object(members) => members.iter().map(|m| (m.key_start, m.value.end)).collect(),
        jsonedit::Kind::Array(items) => items.iter().map(|n| (n.start, n.end)).collect(),
        _ => Vec::new(),
    }
}

fn remove_child(container: &Node, index: usize) -> Splice {
    let spans = child_spans(container);
    let n = spans.len();
    if n == 1 {
        let empty = if container.is_object() { "{}" } else { "[]" };
        return Splice { start: container.start, end: container.end, text: empty.to_string() };
    }
    if index + 1 < n {
        Splice { start: spans[index].0, end: spans[index + 1].0, text: String::new() }
    } else {
        Splice { start: spans[index - 1].1, end: spans[index].1, text: String::new() }
    }
}

fn member_index(container: &Node, key: &str) -> Option<usize> {
    match &container.kind {
        jsonedit::Kind::Object(members) => members.iter().position(|m| m.key == key),
        _ => None,
    }
}

fn is_ours_handler(node: &Node) -> bool {
    node.get("command").and_then(Node::text).is_some_and(is_ours)
}

fn settings_step(root: &Node) -> Option<(Splice, String)> {
    if let Some(hooks) = root.get("hooks")
        && let jsonedit::Kind::Object(events) = &hooks.kind
    {
        for (ei, event) in events.iter().enumerate() {
            let jsonedit::Kind::Array(groups) = &event.value.kind else { continue };
            for (gi, group) in groups.iter().enumerate() {
                let Some(handlers) = group.get("hooks") else { continue };
                for (hi, handler) in handlers.items().iter().enumerate() {
                    if !is_ours_handler(handler) {
                        continue;
                    }
                    let note = format!("hooks.{}: formslot hook removed", event.key);
                    let plain_group = match &group.kind {
                        jsonedit::Kind::Object(m) => m.iter().all(|x| x.key == "matcher" || x.key == "hooks"),
                        _ => false,
                    };
                    if handlers.items().len() != 1 || !plain_group {
                        return Some((remove_child(handlers, hi), note));
                    }
                    if groups.len() != 1 {
                        return Some((remove_child(&event.value, gi), note));
                    }
                    if events.len() != 1 {
                        return Some((remove_child(hooks, ei), note));
                    }
                    let at = member_index(root, "hooks")?;
                    return Some((remove_child(root, at), note));
                }
            }
        }
    }
    if let Some(list) = root.get("enabledMcpjsonServers").filter(|a| a.is_array())
        && let Some(i) = list.items().iter().position(|x| x.text() == Some(SERVER))
    {
        let note = "enabledMcpjsonServers: formslot removed".to_string();
        if list.items().len() == 1 {
            let at = member_index(root, "enabledMcpjsonServers")?;
            return Some((remove_child(root, at), note));
        }
        return Some((remove_child(list, i), note));
    }
    None
}

fn kept_handlers(value: &Value) -> (Vec<Value>, usize) {
    let mut kept = Vec::new();
    let mut ours = 0;
    if let Some(events) = value.get("hooks").and_then(Value::as_object) {
        for groups in events.values().filter_map(Value::as_array) {
            for group in groups {
                for handler in group.get("hooks").and_then(Value::as_array).into_iter().flatten() {
                    if handler.get("command").and_then(Value::as_str).is_some_and(is_ours) {
                        ours += 1;
                    } else {
                        kept.push(handler.clone());
                    }
                }
            }
        }
    }
    (kept, ours)
}

pub fn unmerge_settings(text: &str) -> crate::Result<(String, Vec<String>)> {
    let mut current = text.to_string();
    let mut notes: Vec<String> = Vec::new();
    for _ in 0..256 {
        let root = jsonedit::parse(&current)?;
        if !root.is_object() {
            return Err("the settings file is not a JSON object".to_string());
        }
        let Some((splice, note)) = settings_step(&root) else { break };
        current = jsonedit::apply(&current, vec![splice]);
        if !notes.contains(&note) {
            notes.push(note);
        }
    }
    let before: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let after: Value = serde_json::from_str(&current).map_err(|e| format!("the removal produced invalid JSON: {e}"))?;
    let (Some(b), Some(a)) = (before.as_object(), after.as_object()) else {
        return Err("the settings file is not a JSON object".to_string());
    };
    for (key, old_value) in b {
        if key == "hooks" || key == "enabledMcpjsonServers" {
            continue;
        }
        if a.get(key) != Some(old_value) {
            return Err(format!("the removal would change the value of {key}; nothing was written"));
        }
    }
    let (kept_before, _) = kept_handlers(&before);
    let (kept_after, ours_after) = kept_handlers(&after);
    if kept_before != kept_after || ours_after != 0 {
        return Err("the removal would change hooks that are not formslot's; nothing was written".to_string());
    }
    let others = |v: &Value| -> Vec<Value> {
        v.get("enabledMcpjsonServers")
            .and_then(Value::as_array)
            .map(|x| x.iter().filter(|i| i.as_str() != Some(SERVER)).cloned().collect())
            .unwrap_or_default()
    };
    if others(&before) != others(&after) {
        return Err("the removal would change enabledMcpjsonServers beyond formslot; nothing was written".to_string());
    }
    Ok((current, notes))
}

pub fn unmerge_mcp(text: &str) -> crate::Result<(String, String)> {
    let root = jsonedit::parse(text)?;
    if !root.is_object() {
        return Err("the .mcp.json file is not a JSON object".to_string());
    }
    let Some(servers) = root.get("mcpServers").filter(|m| m.is_object()) else {
        return Ok((text.to_string(), "no formslot entry".to_string()));
    };
    let Some(index) = member_index(servers, SERVER) else {
        return Ok((text.to_string(), "no formslot entry".to_string()));
    };
    let existing = servers.get(SERVER).ok_or_else(|| "no formslot entry".to_string())?;
    let ours = existing.get("command").and_then(Node::text).is_some_and(is_our_program)
        && existing.get("args").is_some_and(|a| a.items().len() == 1 && a.items()[0].text() == Some("mcp"));
    if !ours {
        return Ok((text.to_string(), "the formslot entry was changed by hand, left as it is".to_string()));
    }
    let splice = if child_spans(servers).len() == 1 {
        let at = member_index(&root, "mcpServers").ok_or_else(|| "no formslot entry".to_string())?;
        remove_child(&root, at)
    } else {
        remove_child(servers, index)
    };
    let merged = jsonedit::apply(text, vec![splice]);
    let before: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let after: Value = serde_json::from_str(&merged).map_err(|e| format!("the removal produced invalid JSON: {e}"))?;
    if let (Some(b), Some(a)) = (before.as_object(), after.as_object()) {
        for (k, v) in b {
            if k != "mcpServers" && a.get(k) != Some(v) {
                return Err(format!("the removal would change {k}; nothing was written"));
            }
        }
        let others = |v: &Value| -> Vec<(String, Value)> {
            v.get("mcpServers")
                .and_then(Value::as_object)
                .map(|m| m.iter().filter(|(k, _)| k.as_str() != SERVER).map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default()
        };
        if others(&before) != others(&after) {
            return Err("the removal would change other servers; nothing was written".to_string());
        }
    }
    Ok((merged, "formslot server removed".to_string()))
}

pub struct Disconnect {
    pub lines: Vec<String>,
    pub settings: bool,
    pub mcp: bool,
    pub backups: Vec<PathBuf>,
}

fn planned(path: &Path, remove: impl Fn(&str) -> crate::Result<(String, String)>) -> crate::Result<(Option<String>, String, bool)> {
    let Some(text) = read_optional(path)? else {
        return Ok((None, "no such file".to_string(), false));
    };
    let (plain, shape) = unwrap_text(&text);
    let (out, note) = remove(&plain).map_err(|e| format!("{}: {e}", path.display()))?;
    let out = rewrap(out, &shape);
    let changed = out != text;
    Ok((Some(out), note, changed))
}

fn settings_remove(plain: &str) -> crate::Result<(String, String)> {
    let (out, notes) = unmerge_settings(plain)?;
    let note = if notes.is_empty() { "no formslot entries".to_string() } else { notes.join("; ") };
    Ok((out, note))
}

pub fn connection(repo: &Path) -> crate::Result<(bool, bool)> {
    let (_, _, settings) = planned(&repo.join(".claude").join("settings.json"), settings_remove)?;
    let (_, _, mcp) = planned(&repo.join(".mcp.json"), unmerge_mcp)?;
    Ok((settings, mcp))
}

pub fn backup_name(path: &Path) -> PathBuf {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let mut copy = path.with_file_name(format!("{name}.formslot-backup"));
    if copy.exists() {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        copy = path.with_file_name(format!("{name}.formslot-backup-{stamp}"));
        let mut n = 1;
        while copy.exists() {
            copy = path.with_file_name(format!("{name}.formslot-backup-{stamp}-{n}"));
            n += 1;
        }
    }
    copy
}

pub fn disconnect(repo: &Path, dry_run: bool) -> crate::Result<Disconnect> {
    if !repo.is_dir() {
        return Err(format!("{} is not a folder", repo.display()));
    }
    no_link(&repo.join(".claude"))?;
    let settings_path = repo.join(".claude").join("settings.json");
    let mcp_path = repo.join(".mcp.json");
    let (settings_text, settings_note, settings) = planned(&settings_path, settings_remove)?;
    let (mcp_text, mcp_note, mcp) = planned(&mcp_path, unmerge_mcp)?;
    let verb = if dry_run { "would change" } else { "changed" };
    let mut lines = Vec::new();
    let mut backups = Vec::new();
    for (path, text, note, changed) in [(&settings_path, settings_text, settings_note, settings), (&mcp_path, mcp_text, mcp_note, mcp)] {
        if !changed {
            lines.push(format!("{}: nothing to remove ({note})", path.display()));
            continue;
        }
        lines.push(format!("{}: {verb} ({note})", path.display()));
        if !dry_run && let Some(text) = text {
            let copy = backup_name(path);
            std::fs::copy(path, &copy).map_err(|e| format!("cannot back up {}: {e}", path.display()))?;
            write_atomic(path, &text)?;
            lines.push(format!("  backup saved as {}", copy.display()));
            backups.push(copy);
        }
    }
    Ok(Disconnect { lines, settings, mcp, backups })
}

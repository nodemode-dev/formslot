use std::collections::HashSet;
use std::sync::OnceLock;

use regex::Regex;
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: i64 = 1;
pub const STATUSES: [&str; 5] = ["proposed", "canonical", "deprecated", "declined", "superseded"];
const ROLES: [&str; 6] = ["source", "style", "test", "asset", "doc", "example"];
const PORTS: [&str; 3] = ["verbatim", "adapt", "rename-strings-only"];
const RELATIONS: [&str; 5] = [
    "clone-T1",
    "clone-T2",
    "clone-T3",
    "related-similar",
    "related-same_name",
];

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: String,
    pub role: String,
    pub target: Option<String>,
    pub sha256: String,
    pub lines: Option<i64>,
    pub port: String,
}

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("fixed pattern"))
}

pub fn is_id(text: &str) -> bool {
    static CELL: OnceLock<Regex> = OnceLock::new();
    text.len() <= 80 && re(&CELL, r"^[a-z][a-z0-9-]*(\.[a-z][a-z0-9-]*)+$").is_match(text)
}

fn is_semver(text: &str) -> bool {
    static CELL: OnceLock<Regex> = OnceLock::new();
    re(
        &CELL,
        r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$",
    )
    .is_match(text)
}

pub fn is_sha256(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn is_commit(text: &str) -> bool {
    (7..=64).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn is_kind(text: &str) -> bool {
    static CELL: OnceLock<Regex> = OnceLock::new();
    re(&CELL, r"^[a-z][a-z0-9-]*$").is_match(text)
}

pub fn safe_relative(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains('\0')
        && !path.split('/').any(|part| part.is_empty() || part == ".." || part == ".")
        && !(path.len() >= 2 && path.as_bytes()[1] == b':')
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn normalise(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
            i += 1;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

pub fn count_lines(data: &[u8]) -> i64 {
    if data.is_empty() {
        return 0;
    }
    let newlines = data.iter().filter(|b| **b == b'\n').count() as i64;
    if data.last() == Some(&b'\n') { newlines } else { newlines + 1 }
}

pub fn hash_text(bytes: &[u8]) -> (String, i64) {
    let text = normalise(bytes);
    (sha256_hex(&text), count_lines(&text))
}

pub fn content_hash(files: &[(String, String)]) -> String {
    let mut sorted: Vec<&(String, String)> = files.iter().collect();
    sorted.sort();
    let mut payload = Vec::new();
    for (path, sha) in sorted {
        payload.extend_from_slice(path.as_bytes());
        payload.push(0);
        payload.extend_from_slice(sha.as_bytes());
        payload.push(b'\n');
    }
    sha256_hex(&payload)
}

pub fn text_at<'a>(manifest: &'a Value, keys: &[&str]) -> Option<&'a str> {
    let mut at = manifest;
    for key in keys {
        at = at.get(*key)?;
    }
    at.as_str()
}

pub fn id_of(manifest: &Value) -> &str {
    text_at(manifest, &["id"]).unwrap_or("")
}

pub fn status_of(manifest: &Value) -> &str {
    text_at(manifest, &["status"]).unwrap_or("")
}

pub fn files_of(manifest: &Value) -> Vec<FileEntry> {
    let Some(list) = manifest.get("files").and_then(Value::as_array) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|item| {
            Some(FileEntry {
                path: item.get("path")?.as_str()?.to_string(),
                role: item.get("role").and_then(Value::as_str).unwrap_or("source").to_string(),
                target: item.get("target").and_then(Value::as_str).map(str::to_string),
                sha256: item.get("sha256").and_then(Value::as_str).unwrap_or("").to_string(),
                lines: item.get("lines").and_then(Value::as_i64),
                port: item.get("port").and_then(Value::as_str).unwrap_or("verbatim").to_string(),
            })
        })
        .collect()
}

pub fn depends_on(manifest: &Value) -> Vec<String> {
    manifest
        .get("dependsOn")
        .and_then(|d| d.get("slots"))
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|d| d.get("id").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

pub fn hashed_pairs(manifest: &Value) -> Option<Vec<(String, String)>> {
    let files = files_of(manifest);
    if files.is_empty() || files.iter().any(|f| f.sha256.is_empty()) {
        return None;
    }
    Some(files.into_iter().map(|f| (f.path, f.sha256)).collect())
}

fn need_text(m: &Value, key: &str, errors: &mut Vec<String>) -> Option<String> {
    match m.get(key) {
        Some(Value::String(s)) if !s.trim().is_empty() => Some(s.clone()),
        Some(Value::String(_)) => {
            errors.push(format!("{key} is empty"));
            None
        }
        Some(_) => {
            errors.push(format!("{key} must be a string"));
            None
        }
        None => {
            errors.push(format!("{key} is missing"));
            None
        }
    }
}

pub fn validate(m: &Value) -> Vec<String> {
    let mut errors = Vec::new();
    if !m.is_object() {
        return vec!["the manifest is not a JSON object".to_string()];
    }
    match m.get("schemaVersion").and_then(Value::as_i64) {
        Some(n) if n >= 1 => {
            if n > SCHEMA_VERSION {
                errors.push(format!("schemaVersion {n} is newer than this build understands ({SCHEMA_VERSION})"));
            }
        }
        _ => errors.push("schemaVersion must be an integer of at least 1".to_string()),
    }
    if let Some(id) = need_text(m, "id", &mut errors) {
        if !is_id(&id) {
            errors.push(format!("id '{id}' is not lowercase dotted kebab such as ui.control-rows"));
        }
    }
    need_text(m, "title", &mut errors);
    if let Some(summary) = need_text(m, "summary", &mut errors) {
        if summary.chars().count() > 280 {
            errors.push("summary is longer than 280 characters".to_string());
        }
    }
    if let Some(kind) = need_text(m, "kind", &mut errors) {
        if !is_kind(&kind) {
            errors.push(format!("kind '{kind}' is not lowercase kebab"));
        }
    }
    let status = need_text(m, "status", &mut errors);
    if let Some(status) = &status {
        if !STATUSES.contains(&status.as_str()) {
            errors.push(format!("status '{status}' is not one of {}", STATUSES.join(" | ")));
        }
        if status == "superseded" {
            match m.get("supersededBy").and_then(Value::as_str) {
                Some(id) if is_id(id) => {}
                _ => errors.push("a superseded slot needs supersededBy with a slot id".to_string()),
            }
        }
    }
    if let Some(by) = m.get("supersededBy") {
        if !by.as_str().is_some_and(is_id) {
            errors.push("supersededBy is not a slot id".to_string());
        }
    }
    if let Some(version) = need_text(m, "version", &mut errors) {
        if !is_semver(&version) {
            errors.push(format!("version '{version}' is not SemVer 2.0.0"));
        }
    }
    match m.get("origin") {
        Some(origin) if origin.is_object() => {
            for key in ["repo", "path", "app", "appVersion"] {
                match origin.get(key).and_then(Value::as_str) {
                    Some(s) if !s.is_empty() => {}
                    _ => errors.push(format!("origin.{key} is missing")),
                }
            }
            if let Some(path) = origin.get("path").and_then(Value::as_str) {
                if !safe_relative(path) {
                    errors.push(format!("origin.path '{path}' must be a relative path"));
                }
            }
            if let Some(commit) = origin.get("commit") {
                if !commit.as_str().is_some_and(is_commit) {
                    errors.push("origin.commit must be 7 to 64 lowercase hex characters".to_string());
                }
            }
        }
        _ => errors.push("origin is missing".to_string()),
    }
    match m.get("files").and_then(Value::as_array) {
        Some(list) if !list.is_empty() => {
            let mut seen = HashSet::new();
            for (i, item) in list.iter().enumerate() {
                let label = item
                    .get("path")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("#{i}"));
                match item.get("path").and_then(Value::as_str) {
                    Some(path) if safe_relative(path) => {
                        if !seen.insert(path.to_string()) {
                            errors.push(format!("files: path '{path}' appears twice"));
                        }
                    }
                    Some(path) => errors.push(format!("files {label}: '{path}' must be a relative path without ..")),
                    None => errors.push(format!("files {label}: path is missing")),
                }
                if let Some(target) = item.get("target") {
                    if !target.as_str().is_some_and(safe_relative) {
                        errors.push(format!("files {label}: target must be a relative path without .."));
                    }
                }
                match item.get("role").and_then(Value::as_str) {
                    Some(role) if ROLES.contains(&role) => {}
                    _ => errors.push(format!("files {label}: role must be one of {}", ROLES.join(" | "))),
                }
                match item.get("port").and_then(Value::as_str) {
                    Some(port) if PORTS.contains(&port) => {}
                    _ => errors.push(format!("files {label}: port must be one of {}", PORTS.join(" | "))),
                }
                if let Some(sha) = item.get("sha256") {
                    if !sha.as_str().is_some_and(|s| s.is_empty() || is_sha256(s)) {
                        errors.push(format!("files {label}: sha256 must be 64 lowercase hex characters"));
                    }
                }
                if let Some(lines) = item.get("lines") {
                    if !lines.as_i64().is_some_and(|n| n >= 0) {
                        errors.push(format!("files {label}: lines must be a non-negative integer"));
                    }
                }
                if let Some(r) = item.get("renameables") {
                    if !r.as_array().is_some_and(|a| a.iter().all(Value::is_string)) {
                        errors.push(format!("files {label}: renameables must be a list of strings"));
                    }
                }
            }
        }
        Some(_) => errors.push("files is empty".to_string()),
        None => errors.push("files is missing".to_string()),
    }
    if let Some(hash) = m.get("contentHash") {
        if !hash.as_str().is_some_and(|s| s.is_empty() || is_sha256(s)) {
            errors.push("contentHash must be 64 lowercase hex characters".to_string());
        }
    }
    if let Some(dep) = m.get("dependsOn").and_then(|d| d.get("slots")) {
        match dep.as_array() {
            Some(list) => {
                for item in list {
                    match item.get("id").and_then(Value::as_str) {
                        Some(id) if is_id(id) => {}
                        _ => errors.push("dependsOn.slots: every item needs a slot id".to_string()),
                    }
                }
            }
            None => errors.push("dependsOn.slots must be a list".to_string()),
        }
        if depends_on(m).iter().any(|d| d == id_of(m)) {
            errors.push("a slot cannot depend on itself".to_string());
        }
    }
    if let Some(copies) = m.get("copies") {
        match copies.as_array() {
            Some(list) => {
                for (i, copy) in list.iter().enumerate() {
                    for key in ["repo", "path", "qualified"] {
                        if !copy.get(key).is_some_and(Value::is_string) {
                            errors.push(format!("copies #{i}: {key} is missing"));
                        }
                    }
                    if !copy
                        .get("relation")
                        .and_then(Value::as_str)
                        .is_some_and(|r| RELATIONS.contains(&r))
                    {
                        errors.push(format!("copies #{i}: relation must be one of {}", RELATIONS.join(" | ")));
                    }
                    if !copy.get("score").and_then(Value::as_f64).is_some_and(|s| (0.0..=1.0).contains(&s)) {
                        errors.push(format!("copies #{i}: score must be a number from 0 to 1"));
                    }
                    if !copy
                        .get("lines")
                        .and_then(Value::as_array)
                        .is_some_and(|l| l.len() == 2 && l.iter().all(|n| n.as_i64().is_some_and(|n| n >= 1)))
                    {
                        errors.push(format!("copies #{i}: lines must be [start, end]"));
                    }
                }
            }
            None => errors.push("copies must be a list".to_string()),
        }
    }
    errors
}

pub fn parse(text: &str) -> crate::Result<Value> {
    serde_json::from_str(text).map_err(|e| format!("not valid JSON: {e}"))
}

pub fn check(manifest: &Value) -> crate::Result<()> {
    let errors = validate(manifest);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;

use super::markdown;

pub const MAX_ANCHORS: usize = 16;

static FILE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[A-Za-z0-9_./\-\[\]]+\.(?:ts|tsx|js|mjs|py|rs|css|html|json|toml|md|sql|wgsl|sh|yaml|yml)\b")
        .expect("file pattern")
});
static IDENT_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[A-Za-z_][A-Za-z0-9_]*").expect("identifier pattern"));
static SPAN_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`([^`\n]{1,60})`").expect("span pattern"));

#[derive(Default)]
pub struct Units {
    pub files: HashSet<String>,
    pub by_base: HashMap<String, Vec<String>>,
    pub units: HashMap<String, Vec<(String, String)>>,
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub title: String,
    pub line: usize,
    pub kind: &'static str,
    pub anchors: Vec<String>,
    pub text: String,
    pub logged: Option<String>,
}

fn clean_path(raw: &str) -> String {
    raw.trim()
        .trim_matches('`')
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_matches('`')
        .to_string()
}

fn clean_symbol(raw: &str) -> String {
    let mut text = raw.trim().trim_matches('`').to_string();
    if let Some(at) = text.find(" (") {
        text.truncate(at);
    }
    text.trim_end_matches("()").trim().trim_matches('`').to_string()
}

fn targets(rest: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |value: String| {
        if !value.is_empty() && !out.contains(&value) {
            out.push(value);
        }
    };
    for group in rest.split(" · ") {
        if let Some((path, symbols)) = group.split_once("::") {
            let path = clean_path(path);
            if path.is_empty() || !(path.contains('.') || path.contains('/')) {
                continue;
            }
            for raw in symbols.split([',', ';']) {
                let symbol = clean_symbol(raw);
                if symbol.is_empty() || symbol.contains(' ') {
                    push(path.clone());
                } else {
                    push(format!("{path}#{symbol}"));
                }
            }
        } else {
            for raw in group.split(',') {
                let path = clean_path(raw);
                if path.contains('.') || path.contains('/') {
                    push(path);
                }
            }
        }
    }
    out
}

fn file_tokens(scope: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for found in FILE_PATTERN.find_iter(scope) {
        let token = found.as_str().trim_start_matches("./").to_string();
        if !out.contains(&token) {
            out.push(token);
        }
    }
    out
}

fn directory(path: &str) -> &str {
    path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("")
}

fn join(dir: &str, rest: &str) -> String {
    if dir.is_empty() { rest.to_string() } else { format!("{dir}/{rest}") }
}

fn resolve(token: &str, log_dir: &str, previous: Option<&str>, units: &Units) -> Option<String> {
    let tries = [join(log_dir, token), join(&join(log_dir, "src"), token), token.to_string()];
    for candidate in tries {
        if units.files.contains(&candidate) {
            return Some(candidate);
        }
    }
    let base = token.rsplit('/').next().unwrap_or(token);
    let hits = units.by_base.get(base)?;
    let suffix = format!("/{token}");
    let matching: Vec<&String> = hits
        .iter()
        .filter(|p| p.ends_with(&suffix) || p.as_str() == token)
        .collect();
    if matching.len() == 1 {
        return Some(matching[0].clone());
    }
    if token.contains('/') && !matching.is_empty() {
        let under: Vec<&&String> = matching.iter().filter(|p| p.starts_with(log_dir)).collect();
        if under.len() == 1 {
            return Some((*under[0]).clone());
        }
        return None;
    }
    if token.contains('/') {
        return None;
    }
    if hits.len() == 1 {
        return Some(hits[0].clone());
    }
    if let Some(prev) = previous {
        let dir = directory(prev);
        let near: Vec<&String> = hits.iter().filter(|p| directory(p) == dir).collect();
        if near.len() == 1 {
            return Some(near[0].clone());
        }
    }
    let under: Vec<&String> = hits.iter().filter(|p| p.starts_with(log_dir)).collect();
    if under.len() == 1 {
        return Some(under[0].clone());
    }
    None
}

fn candidate_names(title: &str, body: &str) -> Vec<String> {
    let ident = &*IDENT_PATTERN;
    let span = &*SPAN_PATTERN;
    let mut out: Vec<String> = Vec::new();
    let mut add = |name: &str| {
        if name.len() >= 3 && !out.iter().any(|n| n == name) {
            out.push(name.to_string());
        }
    };
    let code_like = |name: &str| -> bool {
        let bytes: Vec<char> = name.chars().collect();
        let lower_upper = bytes.windows(2).any(|w| w[0].is_ascii_lowercase() && w[1].is_ascii_uppercase());
        let snake = name.contains('_') && name.chars().any(|c| c.is_ascii_alphabetic());
        lower_upper || snake
    };
    let whole = format!("{title}\n{body}");
    for found in ident.find_iter(&whole) {
        if code_like(found.as_str()) {
            add(found.as_str());
        }
    }
    for caps in span.captures_iter(&whole) {
        for found in ident.find_iter(&caps[1]) {
            add(found.as_str());
        }
    }
    out
}

fn normal_symbol(text: &str) -> String {
    text.replace("::", ".")
}

fn legacy_anchors(title: &str, level: usize, body: &str, log_dir: &str, units: &Units) -> Vec<String> {
    let mut scopes: Vec<String> = Vec::new();
    let marker = title.find("_v").or_else(|| title.find(" ("));
    if let Some(at) = marker {
        let after = &title[at..];
        if let Some(open) = after.find('(') {
            scopes.push(after[open + 1..].trim_end_matches(')').to_string());
        }
    }
    if level == 3 {
        if let Some(first) = title.split(" · ").next() {
            scopes.push(first.to_string());
        }
    }
    let mut files: Vec<String> = Vec::new();
    let mut previous: Option<String> = None;
    for scope in &scopes {
        for token in file_tokens(scope) {
            if let Some(path) = resolve(&token, log_dir, previous.as_deref(), units) {
                previous = Some(path.clone());
                if !files.contains(&path) {
                    files.push(path);
                }
            }
        }
    }
    if files.is_empty() {
        return Vec::new();
    }
    let names = candidate_names(title, body);
    let normal_names: Vec<String> = names.iter().map(|n| normal_symbol(n)).collect();
    let mut hits: Vec<(usize, String)> = Vec::new();
    for path in &files {
        let Some(list) = units.units.get(path) else { continue };
        for (qualified, name) in list {
            let wanted = normal_symbol(qualified);
            let rank = names
                .iter()
                .zip(&normal_names)
                .position(|(n, normal)| n == name || *normal == wanted);
            if let Some(rank) = rank {
                let anchor = format!("{path}#{qualified}");
                if !hits.iter().any(|(_, a)| a == &anchor) {
                    hits.push((rank, anchor));
                }
            }
        }
    }
    hits.sort();
    hits.into_iter().take(MAX_ANCHORS).map(|(_, a)| a).collect()
}

pub fn parse(text: &str, log_path: &str, units: &Units) -> Vec<Entry> {
    let version = Regex::new(r"_v\d+\.\d+\.\d+").expect("version pattern");
    let date = Regex::new(r"\d{4}-\d{2}-\d{2}").expect("date pattern");
    let removed = Regex::new(r"(?m)(?:^[-*] |[.;:(] ?|^)Removed\b").expect("removed pattern");
    let log_dir = directory(log_path);
    let mut out: Vec<Entry> = Vec::new();
    for section in markdown::sections(text) {
        if section.level != 2 && section.level != 3 {
            continue;
        }
        let mut refers: Vec<String> = Vec::new();
        let mut kind_line: Option<String> = None;
        let mut logged: Option<String> = None;
        let mut has_refers = false;
        let mut body_lines: Vec<&str> = Vec::new();
        for line in section.body.lines() {
            let trimmed = line.trim_start();
            if let Some(rest) = trimmed.strip_prefix("- refers-to:") {
                has_refers = true;
                for target in targets(rest) {
                    if !refers.contains(&target) {
                        refers.push(target);
                    }
                }
            } else if let Some(rest) = trimmed.strip_prefix("- kind:") {
                kind_line = Some(rest.trim().to_string());
            } else if let Some(rest) = trimmed.strip_prefix("- logged:") {
                logged = date.find(rest).map(|m| m.as_str().to_string());
            } else {
                body_lines.push(line);
            }
        }
        let body = body_lines.join("\n").trim().to_string();
        if body.is_empty() {
            continue;
        }
        let legacy = !has_refers;
        if legacy {
            let qualifies = version.is_match(&section.title)
                || (section.level == 3 && section.title.contains(" · "));
            if !qualifies {
                continue;
            }
        }
        let anchors = if has_refers {
            refers
        } else {
            legacy_anchors(&section.title, section.level, &body, log_dir, units)
        };
        let kind = if has_refers {
            if kind_line.as_deref() == Some("removed-code") { "tombstone" } else { "hashout" }
        } else if removed.is_match(&body) {
            "tombstone"
        } else {
            "hashout"
        };
        out.push(Entry {
            title: section.title.clone(),
            line: section.line,
            kind,
            anchors,
            text: body,
            logged,
        });
    }
    out
}

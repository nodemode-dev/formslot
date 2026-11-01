use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const DEFAULT: &str = include_str!("../../../data/declines.json");

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Spec {
    pub title: String,
    #[serde(default)]
    pub corpus: String,
    pub find: Vec<String>,
    pub pick: Vec<String>,
    pub triggers: String,
}

#[derive(Deserialize)]
struct Sheet {
    specs: Vec<Spec>,
}

#[derive(Debug, Clone)]
pub struct Hit {
    pub title: String,
    pub text: String,
    pub path: String,
    pub line: usize,
    pub triggers: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub title: String,
    pub corpus: String,
    pub expected: bool,
    pub path: Option<String>,
    pub line: Option<usize>,
}

pub fn override_path() -> PathBuf {
    crate::config::data_dir().join("declines.json")
}

pub fn source() -> String {
    let path = override_path();
    if path.is_file() {
        path.display().to_string()
    } else {
        "built-in data/declines.json".to_string()
    }
}

fn parse(text: &str, origin: &str) -> crate::Result<Vec<Spec>> {
    let sheet: Sheet = serde_json::from_str(text).map_err(|e| format!("declines: {origin}: {e}"))?;
    for spec in &sheet.specs {
        if spec.title.trim().is_empty() || spec.find.is_empty() || spec.find.iter().any(|f| f.trim().is_empty()) {
            return Err(format!("declines: {origin}: every spec needs a title and a non-empty find list"));
        }
    }
    Ok(sheet.specs)
}

pub fn specs() -> crate::Result<Vec<Spec>> {
    let path = override_path();
    if path.is_file() {
        let text = fs::read_to_string(&path).map_err(|e| format!("declines: {}: {e}", path.display()))?;
        return parse(&text, &path.display().to_string());
    }
    parse(DEFAULT, "built-in data/declines.json")
}

fn sentences(line: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let mut start = 0usize;
    let mut depth = 0i32;
    for (i, ch) in chars.iter().enumerate() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        let ends = *ch == '.' && depth <= 0 && chars.get(i + 1).is_none_or(|c| c.is_whitespace());
        if ends {
            let piece: String = chars[start..=i].iter().collect();
            out.push(piece.trim().to_string());
            start = i + 1;
        }
    }
    if start < chars.len() {
        let piece: String = chars[start..].iter().collect();
        let piece = piece.trim().to_string();
        if !piece.is_empty() {
            out.push(piece);
        }
    }
    out
}

fn strip_marker(line: &str) -> &str {
    let t = line.trim_start();
    for marker in ["- ", "* ", "+ "] {
        if let Some(rest) = t.strip_prefix(marker) {
            return rest.trim();
        }
    }
    t.trim()
}

fn locate(spec: &Spec, files: &[(String, String)]) -> Option<Hit> {
    let needles: Vec<String> = spec.find.iter().map(|n| n.to_lowercase()).collect();
    for (path, text) in files {
        for (index, raw) in text.split('\n').enumerate() {
            let lower = raw.to_lowercase();
            if !needles.iter().all(|needle| lower.contains(needle.as_str())) {
                continue;
            }
            let body = strip_marker(raw.trim_end_matches('\r'));
            let picks: Vec<String> = spec.pick.iter().map(|p| p.to_lowercase()).collect();
            let picked: Vec<String> = sentences(body)
                .into_iter()
                .filter(|s| {
                    let l = s.to_lowercase();
                    picks.iter().any(|p| l.contains(p.as_str()))
                })
                .collect();
            let text = if picked.is_empty() { body.to_string() } else { picked.join(" ") };
            return Some(Hit {
                title: spec.title.clone(),
                text,
                path: path.clone(),
                line: index + 1,
                triggers: spec.triggers.clone(),
            });
        }
    }
    None
}

pub fn scan(files: &[(String, String)]) -> crate::Result<Vec<Hit>> {
    Ok(specs()?.iter().filter_map(|spec| locate(spec, files)).collect())
}

pub fn check(files: &[(String, String)], corpus: &str) -> crate::Result<Vec<Check>> {
    let mut out = Vec::new();
    for spec in specs()? {
        let expected = spec.corpus.is_empty() || spec.corpus.eq_ignore_ascii_case(corpus);
        let hit = locate(&spec, files);
        out.push(Check {
            title: spec.title.clone(),
            corpus: spec.corpus.clone(),
            expected,
            path: hit.as_ref().map(|h| h.path.clone()),
            line: hit.as_ref().map(|h| h.line),
        });
    }
    Ok(out)
}

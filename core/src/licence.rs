use std::collections::HashMap;
use std::sync::OnceLock;

use regex::Regex;

pub const SOURCE_HEADER: &str = "header";
pub const SOURCE_LICENCE_FILE: &str = "licence_file";
pub const SOURCE_MANIFEST: &str = "manifest";
pub const SOURCE_UNKNOWN: &str = "unknown";

const LICENCE_FILE_NAMES: &[&str] = &[
    "LICENSE",
    "LICENSE.md",
    "LICENSE.txt",
    "LICENCE",
    "LICENCE.md",
    "COPYING",
    "COPYING.md",
    "COPYING.txt",
];

const MANIFEST_NAMES: &[&str] = &["package.json", "Cargo.toml", "pyproject.toml"];

const HEADER_LINES: usize = 30;
const TEXT_WINDOW: usize = 3000;

const LICENCE_TABLE: &[(&str, &[&str])] = &[
    (
        "AGPL-3.0",
        &["gnu affero general public license", "version 3"],
    ),
    (
        "LGPL-3.0",
        &["gnu lesser general public license", "version 3"],
    ),
    (
        "LGPL-2.1",
        &["gnu lesser general public license", "version 2.1"],
    ),
    ("GPL-3.0", &["gnu general public license", "version 3"]),
    ("GPL-2.0", &["gnu general public license", "version 2"]),
    ("Apache-2.0", &["apache license", "version 2.0"]),
    ("MPL-2.0", &["mozilla public license", "2.0"]),
    (
        "BSD-3-Clause",
        &[
            "redistribution and use in source and binary forms",
            "neither the name",
        ],
    ),
    (
        "BSD-2-Clause",
        &["redistribution and use in source and binary forms"],
    ),
    ("MIT", &["permission is hereby granted, free of charge"]),
    (
        "ISC",
        &["permission to use, copy, modify, and/or distribute this software"],
    ),
    (
        "Unlicense",
        &["free and unencumbered software released into the public domain"],
    ),
    ("CC0-1.0", &["cc0 1.0 universal"]),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Licence {
    pub spdx: String,
    pub source: &'static str,
}

impl Licence {
    pub fn unknown() -> Licence {
        Licence {
            spdx: "unknown".to_string(),
            source: SOURCE_UNKNOWN,
        }
    }
}

fn header_regex() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| Regex::new(r"SPDX-License-Identifier:\s*(.+)").expect("header regex"))
}

fn toml_licence_regex() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| {
        Regex::new(r#"(?m)^\s*license\s*=\s*(?:"([^"]+)"|\{\s*text\s*=\s*"([^"]+)")"#)
            .expect("manifest regex")
    })
}

pub fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

pub fn dir_of(path: &str) -> &str {
    match path.rfind('/') {
        Some(i) => &path[..i],
        None => "",
    }
}

pub fn is_licence_file(path: &str) -> bool {
    let name = file_name(path).to_ascii_uppercase();
    LICENCE_FILE_NAMES.iter().any(|n| *n == name)
}

pub fn is_manifest(path: &str) -> bool {
    MANIFEST_NAMES.contains(&file_name(path))
}

pub fn header_spdx(text: &str) -> Option<String> {
    for line in text.lines().take(HEADER_LINES) {
        if let Some(found) = header_regex().captures(line) {
            let raw = found.get(1)?.as_str();
            let cleaned = raw
                .trim()
                .trim_end_matches(|c: char| matches!(c, '*' | '/' | '>' | '-' | '#' | '}' | ' '));
            if !cleaned.is_empty() {
                return Some(cleaned.to_string());
            }
        }
    }
    None
}

pub fn licence_text_spdx(text: &str) -> Option<&'static str> {
    let mut window: String = text.chars().take(TEXT_WINDOW * 2).collect();
    window = window.to_lowercase();
    let normal: String = window.split_whitespace().collect::<Vec<_>>().join(" ");
    let cut: String = normal.chars().take(TEXT_WINDOW).collect();
    LICENCE_TABLE
        .iter()
        .find(|(_, needles)| needles.iter().all(|n| cut.contains(n)))
        .map(|(spdx, _)| *spdx)
}

pub fn manifest_spdx(path: &str, text: &str) -> Option<String> {
    match file_name(path) {
        "package.json" => {
            let value: serde_json::Value = serde_json::from_str(text).ok()?;
            match value.get("license")? {
                serde_json::Value::String(s) if !s.trim().is_empty() => {
                    Some(s.trim().to_string())
                }
                serde_json::Value::Object(o) => {
                    o.get("type")?.as_str().map(|s| s.trim().to_string())
                }
                _ => None,
            }
        }
        "Cargo.toml" | "pyproject.toml" => {
            let found = toml_licence_regex().captures(text)?;
            let value = found.get(1).or_else(|| found.get(2))?;
            Some(value.as_str().trim().to_string())
        }
        _ => None,
    }
}

#[derive(Default)]
pub struct Index {
    files: HashMap<String, String>,
    manifests: HashMap<String, String>,
}

impl Index {
    pub fn new() -> Index {
        Index::default()
    }

    pub fn add_licence_file(&mut self, path: &str, spdx: &str) {
        self.files
            .entry(dir_of(path).to_string())
            .or_insert_with(|| spdx.to_string());
    }

    pub fn add_manifest(&mut self, path: &str, spdx: &str) {
        self.manifests
            .entry(dir_of(path).to_string())
            .or_insert_with(|| spdx.to_string());
    }

    fn nearest<'a>(map: &'a HashMap<String, String>, path: &str) -> Option<&'a String> {
        let mut dir = dir_of(path);
        loop {
            if let Some(found) = map.get(dir) {
                return Some(found);
            }
            if dir.is_empty() {
                return None;
            }
            dir = dir_of(dir);
        }
    }

    pub fn resolve(&self, path: &str, header: Option<&str>) -> Licence {
        if let Some(spdx) = header {
            return Licence {
                spdx: spdx.to_string(),
                source: SOURCE_HEADER,
            };
        }
        if let Some(spdx) = Index::nearest(&self.files, path) {
            return Licence {
                spdx: spdx.clone(),
                source: SOURCE_LICENCE_FILE,
            };
        }
        if let Some(spdx) = Index::nearest(&self.manifests, path) {
            return Licence {
                spdx: spdx.clone(),
                source: SOURCE_MANIFEST,
            };
        }
        Licence::unknown()
    }
}

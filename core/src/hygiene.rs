use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use regex::{Regex, RegexSet};
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub rule: String,
    pub line: u32,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PathFinding {
    pub path: String,
    pub rule: String,
    pub line: u32,
    pub fingerprint: String,
}

const RULES: &[(&str, &str)] = &[
    ("aws-access-key", r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b"),
    ("github-token", r"\bgh[pousr]_[A-Za-z0-9]{36,}\b"),
    ("github-pat", r"\bgithub_pat_[A-Za-z0-9_]{22,}\b"),
    ("slack-token", r"\bxox[abprs]-[A-Za-z0-9-]{10,}"),
    (
        "private-key",
        r"-----BEGIN (?:RSA |EC |DSA |OPENSSH |PGP |ENCRYPTED )?PRIVATE KEY(?: BLOCK)?-----",
    ),
    ("anthropic-key", r"\bsk-ant-[A-Za-z0-9_\-]{20,}"),
    ("openai-key", r"\bsk-(?:proj-)?[A-Za-z0-9_]{32,}"),
    (
        "fal-key",
        r"\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}:[0-9a-f]{32}\b",
    ),
    (
        "generic-api-key",
        r#"(?i)api[_-]?key\s*[:=]\s*['"][A-Za-z0-9_\-]{20,}"#,
    ),
    (
        "hex-near-secret",
        r"(?i)\b(?:secret|token)\b.{0,24}\b[0-9a-f]{40,}\b",
    ),
];

struct Compiled {
    set: RegexSet,
    each: Vec<Regex>,
}

fn compiled() -> &'static Compiled {
    static CELL: OnceLock<Compiled> = OnceLock::new();
    CELL.get_or_init(|| {
        let patterns: Vec<&str> = RULES.iter().map(|(_, p)| *p).collect();
        let set = RegexSet::new(&patterns).expect("secret rules compile");
        let each = patterns
            .iter()
            .map(|p| Regex::new(p).expect("secret rule compiles"))
            .collect();
        Compiled { set, each }
    })
}

pub fn fingerprint(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn scan_text(text: &str) -> Vec<Finding> {
    let compiled = compiled();
    if !compiled.set.is_match(text) {
        return Vec::new();
    }
    let mut findings = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let hits = compiled.set.matches(line);
        if !hits.matched_any() {
            continue;
        }
        for rule_index in hits.iter() {
            for found in compiled.each[rule_index].find_iter(line) {
                findings.push(Finding {
                    rule: RULES[rule_index].0.to_string(),
                    line: (index + 1) as u32,
                    fingerprint: fingerprint(found.as_str()),
                });
            }
        }
    }
    findings
}

pub fn find_gitleaks() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("FORMSLOT_BIN_DIR") {
        let candidate = Path::new(&dir).join("gitleaks");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join("gitleaks"))
        .find(|candidate| candidate.is_file())
}

pub fn scan_with_gitleaks(tree: &Path) -> Result<Vec<PathFinding>, String> {
    let binary = find_gitleaks().ok_or_else(|| "gitleaks is not installed".to_string())?;
    let out = Command::new(binary)
        .arg("dir")
        .arg(tree)
        .args([
            "--report-format",
            "json",
            "--report-path",
            "-",
            "--redact",
            "--no-banner",
            "--log-level",
            "error",
            "--exit-code",
            "0",
        ])
        .output()
        .map_err(|e| format!("cannot run gitleaks: {e}"))?;
    if !out.status.success() {
        let msg = String::from_utf8_lossy(&out.stderr);
        return Err(format!("gitleaks failed: {}", msg.trim()));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let start = text.find('[').unwrap_or(text.len());
    let body = &text[start..];
    if body.trim().is_empty() {
        return Ok(Vec::new());
    }
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(body).map_err(|e| format!("gitleaks report is not JSON: {e}"))?;
    let mut findings = Vec::new();
    for row in rows {
        let rule = row["RuleID"].as_str().unwrap_or("gitleaks").to_string();
        let file = row["File"].as_str().unwrap_or("");
        let line = row["StartLine"].as_u64().unwrap_or(0) as u32;
        let relative = Path::new(file)
            .strip_prefix(tree)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| file.to_string());
        findings.push(PathFinding {
            fingerprint: fingerprint(&format!("{relative}:{rule}:{line}")),
            path: relative,
            rule,
            line,
        });
    }
    Ok(findings)
}

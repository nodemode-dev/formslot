use std::path::{Path, PathBuf};

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::config;
use crate::install::{no_link, write_atomic};

pub struct Part {
    pub path: &'static str,
    pub text: &'static str,
    pub what: &'static str,
}

pub const PARTS: [Part; 3] = [
    Part {
        path: ".claude/skills/formslot-lookup/SKILL.md",
        text: include_str!("../../plugin/formslot/skills/formslot-lookup/SKILL.md"),
        what: "the formslot-lookup skill",
    },
    Part {
        path: ".claude/skills/formslot-curation/SKILL.md",
        text: include_str!("../../plugin/formslot/skills/formslot-curation/SKILL.md"),
        what: "the formslot-curation skill",
    },
    Part {
        path: ".claude/agents/formslot-scout.md",
        text: include_str!("../../plugin/formslot/agents/formslot-scout.md"),
        what: "the formslot-scout agent",
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Missing,
    Current,
    Older,
    Changed,
}

fn sha(text: &str) -> String {
    Sha256::digest(text.replace("\r\n", "\n").as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

fn record_path() -> PathBuf {
    config::data_dir().join("agent_files.json")
}

fn record() -> Map<String, Value> {
    std::fs::read_to_string(record_path())
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default()
}

fn repo_key(repo: &Path) -> String {
    repo.canonicalize().unwrap_or_else(|_| repo.to_path_buf()).to_string_lossy().trim_start_matches(r"\\?\").to_string()
}

fn recorded(repo: &Path, part: &Part) -> Option<String> {
    record().get(&repo_key(repo))?.get(part.path)?.as_str().map(str::to_string)
}

fn remember(repo: &Path, part: &Part, hash: Option<&str>) -> crate::Result<()> {
    let mut all = record();
    let key = repo_key(repo);
    let mut files = all.get(&key).and_then(Value::as_object).cloned().unwrap_or_default();
    match hash {
        Some(h) => {
            files.insert(part.path.to_string(), Value::String(h.to_string()));
        }
        None => {
            files.remove(part.path);
        }
    }
    if files.is_empty() {
        all.remove(&key);
    } else {
        all.insert(key, Value::Object(files));
    }
    write_atomic(&record_path(), &format!("{}\n", Value::Object(all)))
}

fn target(repo: &Path, part: &Part) -> PathBuf {
    part.path.split('/').fold(repo.to_path_buf(), |at, piece| at.join(piece))
}

fn no_links(repo: &Path, part: &Part) -> crate::Result<()> {
    let mut at = repo.to_path_buf();
    for piece in part.path.split('/') {
        at = at.join(piece);
        no_link(&at)?;
    }
    Ok(())
}

pub fn state(repo: &Path, part: &Part) -> State {
    let Ok(found) = std::fs::read_to_string(target(repo, part)) else {
        return State::Missing;
    };
    let hash = sha(&found);
    if hash == sha(part.text) {
        State::Current
    } else if recorded(repo, part).is_some_and(|written| written == hash) {
        State::Older
    } else {
        State::Changed
    }
}

pub fn present(repo: &Path) -> bool {
    PARTS.iter().any(|part| matches!(state(repo, part), State::Current | State::Older))
}

pub fn install(repo: &Path, dry_run: bool, lines: &mut Vec<String>) -> crate::Result<()> {
    for part in &PARTS {
        no_links(repo, part)?;
        let path = target(repo, part);
        let found = state(repo, part);
        let verb = match found {
            State::Current => {
                lines.push(format!("{}: {} is already this version", path.display(), part.what));
                if !dry_run && recorded(repo, part).is_none() {
                    remember(repo, part, Some(&sha(part.text)))?;
                }
                continue;
            }
            State::Changed => {
                lines.push(format!(
                    "{}: {} was changed by hand, left as it is; delete the file and run install again to get this version's",
                    path.display(),
                    part.what
                ));
                continue;
            }
            State::Missing => "written",
            State::Older => "updated to this version",
        };
        if dry_run {
            lines.push(format!("{}: {} would be {verb}", path.display(), part.what));
            continue;
        }
        write_atomic(&path, part.text)?;
        remember(repo, part, Some(&sha(part.text)))?;
        lines.push(format!("{}: {} {verb}", path.display(), part.what));
    }
    Ok(())
}

pub fn remove(repo: &Path, dry_run: bool, lines: &mut Vec<String>) -> crate::Result<()> {
    for part in &PARTS {
        no_links(repo, part)?;
        let path = target(repo, part);
        match state(repo, part) {
            State::Missing => {}
            State::Changed => lines.push(format!("{}: {} was changed by hand, left in place", path.display(), part.what)),
            State::Current | State::Older => {
                if dry_run {
                    lines.push(format!("{}: {} would be removed", path.display(), part.what));
                    continue;
                }
                std::fs::remove_file(&path).map_err(|e| format!("cannot remove {}: {e}", path.display()))?;
                lines.push(format!("{}: {} removed", path.display(), part.what));
                let mut folder = path.parent().map(Path::to_path_buf);
                while let Some(dir) = folder.filter(|d| d != &repo.join(".claude") && d.starts_with(repo.join(".claude"))) {
                    if std::fs::remove_dir(&dir).is_err() {
                        break;
                    }
                    folder = dir.parent().map(Path::to_path_buf);
                }
            }
        }
        if !dry_run {
            remember(repo, part, None)?;
        }
    }
    Ok(())
}

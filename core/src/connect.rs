use std::path::{Path, PathBuf};

use crate::install::{self, no_link, write_atomic};

const HEADER: &str = "[mcp_servers.formslot]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Agent {
    Codex,
    Cursor,
    Antigravity,
}

impl Agent {
    pub fn parse(name: &str) -> Option<Agent> {
        match name {
            "codex" => Some(Agent::Codex),
            "cursor" => Some(Agent::Cursor),
            "antigravity" => Some(Agent::Antigravity),
            _ => None,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Agent::Codex => "Codex",
            Agent::Cursor => "Cursor",
            Agent::Antigravity => "Antigravity",
        }
    }

    pub fn program(self) -> &'static str {
        match self {
            Agent::Codex => "codex",
            Agent::Cursor => "cursor-agent",
            Agent::Antigravity => "agy",
        }
    }

    pub fn project_file(self, repo: &Path) -> PathBuf {
        match self {
            Agent::Codex => repo.join(".codex").join("config.toml"),
            Agent::Cursor => repo.join(".cursor").join("mcp.json"),
            Agent::Antigravity => repo.join(".agents").join("mcp_config.json"),
        }
    }

    pub fn user_file(self, home: &Path) -> PathBuf {
        match self {
            Agent::Codex => std::env::var_os("CODEX_HOME").filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or_else(|| home.join(".codex")).join("config.toml"),
            Agent::Cursor => home.join(".cursor").join("mcp.json"),
            Agent::Antigravity => home.join(".gemini").join("config").join("mcp_config.json"),
        }
    }
}

pub const AGENTS: [Agent; 3] = [Agent::Codex, Agent::Cursor, Agent::Antigravity];

fn read(path: &Path) -> crate::Result<Option<String>> {
    no_link(path)?;
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("cannot read {}: {e}", path.display())),
    }
}

fn block() -> String {
    let command = serde_json::to_string(&install::launcher_path()).unwrap_or_else(|_| "\"formslot\"".to_string());
    format!("{HEADER}\ncommand = {command}\nargs = [\"mcp\"]\n")
}

fn section(text: &str) -> Option<(usize, usize)> {
    let mut at = 0;
    let mut start = None;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        match start {
            None if trimmed == HEADER => start = Some(at),
            Some(from) if trimmed.starts_with('[') => return Some((from, at)),
            _ => {}
        }
        at += line.len();
    }
    start.map(|from| (from, text.len()))
}

fn section_is_ours(body: &str) -> bool {
    body.lines().skip(1).map(str::trim).filter(|l| !l.is_empty()).all(|l| l.starts_with("command =") || l == "args = [\"mcp\"]")
}

fn toml_add(text: Option<&str>) -> (String, &'static str) {
    let Some(text) = text else {
        return (block(), "created with the formslot server");
    };
    if section(text).is_some() {
        return (text.to_string(), "formslot server already defined, left as it is");
    }
    let gap = if text.is_empty() || text.ends_with("\n\n") {
        ""
    } else if text.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    (format!("{text}{gap}{}", block()), "formslot server added")
}

fn toml_remove(text: &str) -> (String, &'static str) {
    let Some((from, to)) = section(text) else {
        return (text.to_string(), "no formslot entry");
    };
    if !section_is_ours(&text[from..to]) {
        return (text.to_string(), "the formslot entry was changed by hand, left as it is");
    }
    let mut kept = format!("{}{}", &text[..from], &text[to..]);
    while kept.ends_with("\n\n") {
        kept.pop();
    }
    (kept, "formslot server removed")
}

pub fn connect(agent: Agent, path: &Path, remove: bool, dry_run: bool) -> crate::Result<String> {
    let found = read(path)?;
    let (fresh, note): (String, String) = match (agent, remove, found.as_deref()) {
        (_, true, None) => return Ok(format!("{}: nothing to remove (no file)", path.display())),
        (Agent::Codex, true, Some(text)) => {
            let (fresh, note) = toml_remove(text);
            (fresh, note.to_string())
        }
        (Agent::Codex, false, text) => {
            let (fresh, note) = toml_add(text);
            (fresh, note.to_string())
        }
        (_, true, Some(text)) => install::unmerge_mcp(text).map_err(|e| format!("{}: {e}", path.display()))?,
        (_, false, text) => install::merge_mcp(text).map_err(|e| format!("{}: {e}", path.display()))?,
    };
    if found.as_deref() == Some(fresh.as_str()) {
        return Ok(format!("{}: nothing to change ({note})", path.display()));
    }
    if dry_run {
        return Ok(format!("{}: would change ({note})", path.display()));
    }
    write_atomic(path, &fresh)?;
    Ok(format!("{}: changed ({note})", path.display()))
}

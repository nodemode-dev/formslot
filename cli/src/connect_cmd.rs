use std::path::PathBuf;
use std::process::ExitCode;

use formslot_core::connect::{self, Agent};

const USAGE: &str = "formslot connect codex|cursor|antigravity [<repo>] [--user] [--remove] [--dry-run]
  adds the formslot MCP server to one agent program's own configuration and changes nothing else in the file
  <repo>     the project to connect (the current folder when left out): .codex/config.toml, .cursor/mcp.json or .agents/mcp_config.json
  --user     the file for every project instead: ~/.codex/config.toml, ~/.cursor/mcp.json or ~/.gemini/config/mcp_config.json
  --remove   take the formslot entry out again; an entry changed by hand is left
  --dry-run  say what would change and write nothing
Claude Code is connected with `formslot install <repo>`, which also brings the hooks, the skill and the scout agent.";

fn fail(message: String) -> ExitCode {
    eprintln!("formslot: {message}");
    ExitCode::FAILURE
}

pub fn run(args: &[String]) -> ExitCode {
    let mut agent: Option<Agent> = None;
    let mut repo: Option<PathBuf> = None;
    let (mut user, mut remove, mut dry_run) = (false, false, false);
    for arg in args {
        match arg.as_str() {
            "--user" => user = true,
            "--remove" => remove = true,
            "--dry-run" => dry_run = true,
            other if other.starts_with("--") => {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
            other if agent.is_none() => match Agent::parse(other) {
                Some(found) => agent = Some(found),
                None => {
                    eprintln!("{USAGE}");
                    return ExitCode::from(2);
                }
            },
            other if repo.is_none() => repo = Some(PathBuf::from(other)),
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    let Some(agent) = agent else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    if user && repo.is_some() {
        return fail("give a project folder or --user, not both".to_string());
    }
    let path = if user {
        let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).filter(|v| !v.is_empty()) else {
            return fail("the home folder is not known (HOME is not set)".to_string());
        };
        agent.user_file(&PathBuf::from(home))
    } else {
        let folder = match repo.map(Ok).unwrap_or_else(std::env::current_dir) {
            Ok(folder) => folder,
            Err(e) => return fail(format!("the current folder could not be read ({e})")),
        };
        if !folder.is_dir() {
            return fail(format!("{} is not a folder", folder.display()));
        }
        agent.project_file(&folder)
    };
    match connect::connect(agent, &path, remove, dry_run) {
        Ok(line) => {
            println!("{line}");
            if !remove && !dry_run {
                println!("{} reads this file the next time it starts; approve the formslot server there if it asks.", agent.title());
            }
            ExitCode::SUCCESS
        }
        Err(e) => fail(e),
    }
}

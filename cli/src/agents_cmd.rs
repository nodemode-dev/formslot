use std::path::{Path, PathBuf};
use std::process::ExitCode;

use formslot_core::mcp::{self, schema};
use serde_json::{Map, Value, json};

const FILE: &str = "agents/manifest.json";
const SKILL_ROOTS: [&str; 2] = ["plugin/formslot/skills", ".claude/skills"];

fn fail(message: String) -> ExitCode {
    eprintln!("formslot: {message}");
    ExitCode::FAILURE
}

fn checkout() -> Option<PathBuf> {
    let start = std::env::current_dir().ok()?;
    start.ancestors().find(|dir| dir.join("Cargo.toml").is_file() && dir.join("plugin").join("formslot").is_dir()).map(Path::to_path_buf)
}

fn folders(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect();
    names.sort();
    names
}

fn skills(root: &Path) -> Vec<Value> {
    let mut found: Vec<Value> = Vec::new();
    for base in SKILL_ROOTS {
        let base = base.split('/').fold(root.to_path_buf(), |at, piece| at.join(piece));
        for name in folders(&base) {
            if base.join(&name).join("SKILL.md").is_file() {
                found.push(json!({ "name": name, "sub_skills": folders(&base.join(&name).join("sub-skills")) }));
            }
        }
    }
    found.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    found
}

fn manifest(root: &Path) -> String {
    let tools = schema::tools();
    let read_only = schema::read_only_names();
    let names = schema::names();
    let own_store: Vec<&String> = names.iter().filter(|name| !read_only.contains(name)).collect();
    let titles: Map<String, Value> = tools
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|tool| Some((tool.get("name")?.as_str()?.to_string(), tool.get("title")?.clone())))
        .collect();
    let value = json!({
        "format": "form-agent-manifest",
        "version": 1,
        "app": "formslot",
        "mcp": {
            "tools": names,
            "titles": titles,
            "read_only": read_only,
            "approval_gated": [],
            "own_store": own_store,
            "transports": ["stdio", "http"],
            "protocols": mcp::supported(),
        },
        "skills": skills(root),
        "dock": { "present": false, "positions": [], "tabs": [] },
        "extensions": null,
    });
    format!("{}\n", serde_json::to_string_pretty(&value).unwrap_or_default())
}

pub fn run(args: &[String]) -> ExitCode {
    let check = match args {
        [sync] if sync == "sync" => false,
        [sync, flag] if sync == "sync" && flag == "--check" => true,
        _ => {
            eprintln!("formslot agents sync [--check]   write agents/manifest.json from the MCP tool definitions and the skill folders; --check changes nothing and exits 1 while the file differs");
            return ExitCode::from(2);
        }
    };
    let Some(root) = checkout() else {
        return fail("agents sync runs inside a formslot source checkout".to_string());
    };
    let path = FILE.split('/').fold(root.clone(), |at, piece| at.join(piece));
    let fresh = manifest(&root);
    let current = std::fs::read_to_string(&path).unwrap_or_default().replace("\r\n", "\n");
    if current == fresh {
        println!("{FILE} is up to date ({} tools)", schema::names().len());
        return ExitCode::SUCCESS;
    }
    if check {
        return fail(format!("{FILE} differs from the tool definitions in core/src/mcp/schema.rs or from the skill folders; run `formslot agents sync` and commit the file"));
    }
    if let Some(dir) = path.parent()
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        return fail(format!("cannot create {}: {e}", dir.display()));
    }
    match std::fs::write(&path, fresh) {
        Ok(()) => {
            println!("wrote {FILE} ({} tools)", schema::names().len());
            ExitCode::SUCCESS
        }
        Err(e) => fail(format!("cannot write {}: {e}", path.display())),
    }
}

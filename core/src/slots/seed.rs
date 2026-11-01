use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::slots::{manifest, storage};
use crate::store::Store;

#[derive(Debug, Clone, Serialize)]
pub struct SeedLine {
    pub file: String,
    pub id: String,
    pub outcome: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct SeedReport {
    pub imported: usize,
    pub unchanged: usize,
    pub skipped: usize,
    pub rejected: usize,
    pub lines: Vec<SeedLine>,
}

const IGNORED: [&str; 3] = ["slots.index.json", "slots.lock.json", "slot.schema.json"];

fn collect(dir: &Path, out: &mut Vec<PathBuf>) -> crate::Result<()> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("cannot read {}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    for path in entries {
        let kind = std::fs::symlink_metadata(&path)
            .map_err(|e| format!("cannot stat {}: {e}", path.display()))?
            .file_type();
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            collect(&path, out)?;
        } else if path.extension().is_some_and(|x| x == "json")
            && !path
                .file_name()
                .is_some_and(|n| IGNORED.contains(&n.to_string_lossy().as_ref()))
        {
            out.push(path);
        }
    }
    Ok(())
}

pub fn seed(store: &mut Store, dir: &Path) -> crate::Result<SeedReport> {
    if !dir.is_dir() {
        return Err(format!("{} is not a folder", dir.display()));
    }
    let mut files = Vec::new();
    collect(dir, &mut files)?;
    if files.is_empty() {
        return Err(format!("no .json manifests under {}", dir.display()));
    }
    let mut report = SeedReport::default();
    for path in files {
        let shown = path.strip_prefix(dir).unwrap_or(&path).display().to_string();
        let mut line = SeedLine {
            file: shown,
            id: String::new(),
            outcome: String::new(),
            detail: String::new(),
        };
        let value = match std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read: {e}"))
            .and_then(|text| manifest::parse(&text))
        {
            Ok(value) => value,
            Err(e) => {
                line.outcome = "rejected".to_string();
                line.detail = e;
                report.rejected += 1;
                report.lines.push(line);
                continue;
            }
        };
        line.id = manifest::id_of(&value).to_string();
        let errors = manifest::validate(&value);
        if !errors.is_empty() {
            line.outcome = "rejected".to_string();
            line.detail = errors.join("; ");
            report.rejected += 1;
            report.lines.push(line);
            continue;
        }
        if manifest::status_of(&value) != "proposed" {
            line.outcome = "rejected".to_string();
            line.detail = format!(
                "status is '{}'; seeds arrive as proposed and only the owner confirms",
                manifest::status_of(&value)
            );
            report.rejected += 1;
            report.lines.push(line);
            continue;
        }
        match storage::get(store.conn(), &line.id)? {
            Some(existing) => {
                if existing.manifest == value {
                    line.outcome = "unchanged".to_string();
                    report.unchanged += 1;
                } else {
                    line.outcome = "skipped".to_string();
                    line.detail = format!(
                        "already stored as {}; use slot propose to replace a proposal",
                        existing.status
                    );
                    report.skipped += 1;
                }
            }
            None => {
                let mut value = value.clone();
                if let Some(object) = value.as_object_mut() {
                    object.insert("copies".to_string(), serde_json::Value::Array(Vec::new()));
                }
                storage::save(store, &value, "seed", "seed", &line.file)?;
                line.outcome = "imported".to_string();
                report.imported += 1;
            }
        }
        report.lines.push(line);
    }
    Ok(report)
}

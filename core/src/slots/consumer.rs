use std::collections::{HashMap, HashSet};
use std::path::Path;

use rusqlite::Connection;
use serde::Serialize;

use crate::corpus::now_iso;
use crate::slots::lock::{self, Lock, LockFile, LockSlot};
use crate::slots::manifest::{self, FileEntry};
use crate::slots::storage::{self, SlotRow};
use crate::store::Store;

const SKIPPED_DIRS: [&str; 7] = [".git", "node_modules", "target", "dist", ".venv", "venv", "__pycache__"];
const REFUSED: [&str; 3] = ["deprecated", "declined", "superseded"];

#[derive(Debug, Clone, Serialize)]
pub struct PlannedFile {
    pub origin_path: String,
    pub target: String,
    pub resolved_by: String,
    pub exists: bool,
    pub role: String,
    pub port: String,
    pub origin_sha256: String,
    pub origin_lines: Option<i64>,
    pub installed_sha256: String,
    pub installed_lines: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlannedSlot {
    pub id: String,
    pub version: String,
    pub status: String,
    pub title: String,
    pub origin_commit: String,
    pub content_hash: String,
    pub depends_on: Vec<String>,
    pub pulled_in_by: Option<String>,
    pub files: Vec<PlannedFile>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    pub repo: String,
    pub slots: Vec<PlannedSlot>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Applied {
    pub plan: Plan,
    pub lockfile: String,
    pub wrote: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DriftFile {
    pub path: String,
    pub origin_path: String,
    pub state: String,
    pub installed_lines: Option<i64>,
    pub origin_lines: Option<i64>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DriftSlot {
    pub id: String,
    pub lock_version: String,
    pub current_version: Option<String>,
    pub status: Option<String>,
    pub state: String,
    pub files: Vec<DriftFile>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InferRow {
    pub file: String,
    pub slot: String,
    pub slot_path: String,
    pub state: String,
    pub file_lines: i64,
    pub slot_lines: Option<i64>,
    pub file_sha256: String,
    pub slot_sha256: String,
}

fn walk_into(root: &Path, dir: &Path, out: &mut Vec<String>) -> crate::Result<()> {
    let read = std::fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
    let mut entries: Vec<_> = read.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().to_string();
        let kind = match entry.file_type() {
            Ok(kind) => kind,
            Err(_) => continue,
        };
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            if !SKIPPED_DIRS.contains(&name.as_str()) {
                walk_into(root, &entry.path(), out)?;
            }
        } else if kind.is_file() {
            if let Ok(rel) = entry.path().strip_prefix(root) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    Ok(())
}

pub fn walk(repo: &Path) -> crate::Result<Vec<String>> {
    if !repo.is_dir() {
        return Err(format!("{} is not a folder", repo.display()));
    }
    let mut out = Vec::new();
    walk_into(repo, repo, &mut out)?;
    Ok(out)
}

fn base_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn by_name(files: &[String]) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for file in files {
        map.entry(base_name(file).to_string()).or_default().push(file.clone());
    }
    map
}

fn suffix_score(a: &str, b: &str) -> usize {
    a.rsplit('/').zip(b.rsplit('/')).take_while(|(x, y)| x == y).count()
}

fn hash_in(repo: &Path, rel: &str) -> Option<(String, i64)> {
    if !manifest::safe_relative(rel) {
        return None;
    }
    let path = repo.join(rel);
    let meta = std::fs::symlink_metadata(&path).ok()?;
    if !meta.file_type().is_file() {
        return None;
    }
    let data = std::fs::read(&path).ok()?;
    Some(manifest::hash_text(&data))
}

fn resolve(
    files: &HashSet<&str>,
    names: &HashMap<String, Vec<String>>,
    entry: &FileEntry,
) -> (String, &'static str) {
    if let Some(target) = &entry.target {
        return (target.clone(), "target");
    }
    if files.contains(entry.path.as_str()) {
        return (entry.path.clone(), "path");
    }
    if let Some(candidates) = names.get(base_name(&entry.path)) {
        let best = candidates.iter().map(|c| suffix_score(c, &entry.path)).max().unwrap_or(0);
        let top: Vec<&String> = candidates
            .iter()
            .filter(|c| suffix_score(c, &entry.path) == best)
            .collect();
        if top.len() == 1 {
            return (top[0].clone(), "name");
        }
    }
    (entry.path.clone(), "none")
}

fn reason_for(conn: &Connection, row: &SlotRow) -> String {
    let manifest = &row.manifest;
    let mut parts = Vec::new();
    if let Some(reason) = manifest::text_at(manifest, &["statusReason"]) {
        parts.push(reason.to_string());
    } else if row.status == "declined" {
        if let Ok(events) = storage::events(conn, &row.id) {
            if let Some(event) = events.iter().rev().find(|e| e.action == "decline") {
                parts.push(event.note.clone());
            }
        }
    }
    if let Some(by) = manifest::text_at(manifest, &["supersededBy"]) {
        parts.push(format!("use {by}"));
    }
    if parts.is_empty() { "no reason recorded".to_string() } else { parts.join("; ") }
}

fn order(conn: &Connection, ids: &[String]) -> crate::Result<Vec<(SlotRow, Option<String>)>> {
    fn visit(
        conn: &Connection,
        id: &str,
        parent: Option<&str>,
        done: &mut HashSet<String>,
        active: &mut Vec<String>,
        out: &mut Vec<(SlotRow, Option<String>)>,
    ) -> crate::Result<()> {
        if done.contains(id) {
            return Ok(());
        }
        if active.iter().any(|a| a == id) {
            return Err(format!("dependsOn has a cycle: {} -> {id}", active.join(" -> ")));
        }
        let row = match storage::get(conn, id)? {
            Some(row) => row,
            None => {
                return Err(match parent {
                    Some(p) => format!("{p} depends on {id}, which is not in the store"),
                    None => format!("no slot with id {id}"),
                });
            }
        };
        active.push(id.to_string());
        for dep in manifest::depends_on(&row.manifest) {
            visit(conn, &dep, Some(id), done, active, out)?;
        }
        active.pop();
        done.insert(id.to_string());
        out.push((row, parent.map(str::to_string)));
        Ok(())
    }
    let mut done = HashSet::new();
    let mut out = Vec::new();
    for id in ids {
        visit(conn, id, None, &mut done, &mut Vec::new(), &mut out)?;
    }
    Ok(out)
}

pub fn plan(conn: &Connection, repo: &Path, ids: &[String], force: bool) -> crate::Result<Plan> {
    if ids.is_empty() {
        return Err("name at least one slot id".to_string());
    }
    let ordered = order(conn, ids)?;
    let mut refused = Vec::new();
    for (row, _) in &ordered {
        if REFUSED.contains(&row.status.as_str()) {
            refused.push(format!("{} is {}: {}", row.id, row.status, reason_for(conn, row)));
        }
    }
    let mut warnings = Vec::new();
    if !refused.is_empty() {
        if !force {
            return Err(format!("{}\nnothing planned; --force overrides", refused.join("\n")));
        }
        for line in refused {
            warnings.push(format!("forced: {line}"));
        }
    }
    let listing = walk(repo)?;
    let present: HashSet<&str> = listing.iter().map(String::as_str).collect();
    let names = by_name(&listing);
    let mut slots = Vec::new();
    for (row, parent) in ordered {
        if row.status == "proposed" {
            warnings.push(format!("{} is proposed: the owner has not confirmed it", row.id));
        }
        let entries = manifest::files_of(&row.manifest);
        if entries.iter().any(|f| f.sha256.is_empty()) {
            return Err(format!("{} has files without sha256; run slot refresh first", row.id));
        }
        let mut files = Vec::new();
        for entry in &entries {
            let (target, how) = resolve(&present, &names, entry);
            let installed = hash_in(repo, &target);
            files.push(PlannedFile {
                origin_path: entry.path.clone(),
                resolved_by: how.to_string(),
                exists: installed.is_some(),
                role: entry.role.clone(),
                port: entry.port.clone(),
                origin_sha256: entry.sha256.clone(),
                origin_lines: entry.lines,
                installed_sha256: installed.as_ref().map(|i| i.0.clone()).unwrap_or_default(),
                installed_lines: installed.map(|i| i.1),
                target,
            });
        }
        slots.push(PlannedSlot {
            title: manifest::text_at(&row.manifest, &["title"]).unwrap_or("").to_string(),
            origin_commit: manifest::text_at(&row.manifest, &["origin", "commit"]).unwrap_or("").to_string(),
            depends_on: manifest::depends_on(&row.manifest),
            id: row.id.clone(),
            version: row.version.clone(),
            status: row.status.clone(),
            content_hash: row.content_hash.clone(),
            pulled_in_by: parent,
            files,
        });
    }
    Ok(Plan { repo: repo.display().to_string(), slots, warnings })
}

pub fn apply(
    store: &mut Store,
    repo: &Path,
    ids: &[String],
    force: bool,
    dry_run: bool,
) -> crate::Result<Applied> {
    let planned = plan(store.conn(), repo, ids, force)?;
    let mut lock = lock::read(repo)?.unwrap_or_else(Lock::empty);
    let mut notes = Vec::new();
    for slot in &planned.slots {
        let previous = lock.slots.iter().position(|s| s.id == slot.id);
        let pinned = previous.map(|i| lock.slots[i].pinned).unwrap_or(false);
        if pinned {
            notes.push(format!("{} is pinned in the lockfile; its entry is refreshed, not moved to a new version", slot.id));
        }
        let mut files = Vec::new();
        for file in &slot.files {
            let origin_sha = if !file.exists || file.installed_sha256 == file.origin_sha256 {
                file.origin_sha256.clone()
            } else {
                notes.push(format!(
                    "{}: {} differs from the origin ({} lines here, {} there); its base release is unknown, so it is recorded as behind",
                    slot.id,
                    file.target,
                    file.installed_lines.unwrap_or(0),
                    file.origin_lines.unwrap_or(0)
                ));
                String::new()
            };
            if !file.exists {
                notes.push(format!(
                    "{}: {} is not in the repository; formslot does not copy it, put it there by hand",
                    slot.id, file.target
                ));
            }
            files.push(LockFile {
                path: file.target.clone(),
                origin_path: file.origin_path.clone(),
                origin_sha256: origin_sha,
                installed_sha256: file.installed_sha256.clone(),
                port: file.port.clone(),
            });
        }
        let entry = LockSlot {
            id: slot.id.clone(),
            version: slot.version.clone(),
            origin_commit: slot.origin_commit.clone(),
            installed_at: now_iso(),
            content_hash: slot.content_hash.clone(),
            files,
            pinned,
        };
        match previous {
            Some(i) => lock.slots[i] = entry,
            None => lock.slots.push(entry),
        }
    }
    let path = lock::path_in(repo);
    if !dry_run {
        lock::write(repo, &lock)?;
        for slot in &planned.slots {
            storage::log_event(store.conn(), &slot.id, "apply", "cli", &planned.repo)?;
        }
    } else {
        notes.push(format!("dry run: {} would hold:\n{}", path.display(), lock::render(&lock)?.trim_end()));
    }
    Ok(Applied { plan: planned, lockfile: path.display().to_string(), wrote: !dry_run, notes })
}

fn state_of(local: bool, origin: bool) -> &'static str {
    match (local, origin) {
        (false, false) => "clean",
        (true, false) => "local",
        (false, true) => "behind",
        (true, true) => "diverged",
    }
}

pub fn drift_lock(conn: &Connection, repo: &Path, lock: &Lock) -> crate::Result<Vec<DriftSlot>> {
    let mut out = Vec::new();
    for entry in &lock.slots {
        let row = storage::get(conn, &entry.id)?;
        let current = row.as_ref().map(|r| manifest::files_of(&r.manifest)).unwrap_or_default();
        let mut files = Vec::new();
        let mut local_any = false;
        let mut origin_any = false;
        let mut missing_any = false;
        for file in &entry.files {
            let now = hash_in(repo, &file.path);
            let origin = current.iter().find(|f| f.path == file.origin_path);
            let mut note = String::new();
            let state = match (&now, &row) {
                (_, None) => {
                    note = "the slot is not in the store".to_string();
                    "unknown"
                }
                (None, _) => {
                    missing_any = true;
                    "missing"
                }
                (Some((sha, _)), Some(_)) => {
                    let local = *sha != file.installed_sha256;
                    let origin_changed = match origin {
                        None => {
                            note = "the slot no longer lists this file".to_string();
                            true
                        }
                        Some(o) if o.sha256.is_empty() => {
                            note = "the slot has no hash for this file; refresh it".to_string();
                            false
                        }
                        Some(o) => o.sha256 != file.origin_sha256,
                    };
                    if file.origin_sha256.is_empty() && origin.is_some_and(|o| !o.sha256.is_empty()) {
                        note = "installed from an older release (base hash unknown)".to_string();
                    }
                    if file.installed_sha256.is_empty() {
                        note = "was not in the repository when recorded".to_string();
                    }
                    local_any |= local;
                    origin_any |= origin_changed;
                    state_of(local, origin_changed)
                }
            };
            files.push(DriftFile {
                path: file.path.clone(),
                origin_path: file.origin_path.clone(),
                state: state.to_string(),
                installed_lines: now.map(|n| n.1),
                origin_lines: origin.and_then(|o| o.lines),
                note,
            });
        }
        let slot_state = if row.is_none() {
            "unknown"
        } else if local_any && origin_any {
            "diverged"
        } else if local_any {
            "local"
        } else if origin_any {
            "behind"
        } else if missing_any {
            "missing"
        } else {
            "clean"
        };
        out.push(DriftSlot {
            id: entry.id.clone(),
            lock_version: entry.version.clone(),
            current_version: row.as_ref().map(|r| r.version.clone()),
            status: row.as_ref().map(|r| r.status.clone()),
            state: slot_state.to_string(),
            files,
        });
    }
    Ok(out)
}

pub fn drift_infer(conn: &Connection, repo: &Path) -> crate::Result<Vec<InferRow>> {
    let listing = walk(repo)?;
    let names = by_name(&listing);
    let mut rows = Vec::new();
    for slot in storage::list(conn, None)? {
        for entry in manifest::files_of(&slot.manifest) {
            let Some(candidates) = names.get(base_name(&entry.path)) else {
                continue;
            };
            let best = candidates.iter().map(|c| suffix_score(c, &entry.path)).max().unwrap_or(0);
            for candidate in candidates.iter().filter(|c| suffix_score(c, &entry.path) == best) {
                let Some((sha, lines)) = hash_in(repo, candidate) else {
                    continue;
                };
                let state = if entry.sha256.is_empty() {
                    "unknown"
                } else if entry.sha256 == sha {
                    "identical"
                } else {
                    "drifted"
                };
                rows.push(InferRow {
                    file: candidate.clone(),
                    slot: slot.id.clone(),
                    slot_path: entry.path.clone(),
                    state: state.to_string(),
                    file_lines: lines,
                    slot_lines: entry.lines,
                    file_sha256: sha,
                    slot_sha256: entry.sha256.clone(),
                });
            }
        }
    }
    rows.sort_by(|a, b| a.file.cmp(&b.file).then(a.slot.cmp(&b.slot)));
    Ok(rows)
}

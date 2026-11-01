use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const FILE_NAME: &str = "slots.lock.json";
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lock {
    pub schema_version: u32,
    pub slots: Vec<LockSlot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockSlot {
    pub id: String,
    pub version: String,
    pub origin_commit: String,
    pub installed_at: String,
    pub content_hash: String,
    pub files: Vec<LockFile>,
    pub pinned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockFile {
    pub path: String,
    pub origin_path: String,
    pub origin_sha256: String,
    pub installed_sha256: String,
    pub port: String,
}

impl Lock {
    pub fn empty() -> Lock {
        Lock { schema_version: SCHEMA_VERSION, slots: Vec::new() }
    }
}

pub fn path_in(repo: &Path) -> PathBuf {
    repo.join(FILE_NAME)
}

pub fn read(repo: &Path) -> crate::Result<Option<Lock>> {
    let path = path_in(repo);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    let lock: Lock =
        serde_json::from_str(&text).map_err(|e| format!("{} is not a valid lockfile: {e}", path.display()))?;
    if lock.schema_version > SCHEMA_VERSION {
        return Err(format!(
            "{} has schemaVersion {}, newer than this build understands ({SCHEMA_VERSION})",
            path.display(),
            lock.schema_version
        ));
    }
    Ok(Some(lock))
}

pub fn render(lock: &Lock) -> crate::Result<String> {
    let mut text = serde_json::to_string_pretty(lock).map_err(|e| format!("lockfile: {e}"))?;
    text.push('\n');
    Ok(text)
}

pub fn write(repo: &Path, lock: &Lock) -> crate::Result<PathBuf> {
    let path = path_in(repo);
    let temp = repo.join(format!("{FILE_NAME}.tmp"));
    if std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(format!("refused: {} is a symbolic link", path.display()));
    }
    if std::fs::symlink_metadata(&temp).is_ok() {
        std::fs::remove_file(&temp).map_err(|e| format!("cannot remove {}: {e}", temp.display()))?;
    }
    {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| format!("cannot write {}: {e}", temp.display()))?;
        file.write_all(render(lock)?.as_bytes())
            .map_err(|e| format!("cannot write {}: {e}", temp.display()))?;
    }
    std::fs::rename(&temp, &path).map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        format!("cannot write {}: {e}", path.display())
    })?;
    Ok(path)
}

use std::env;
use std::path::{Path, PathBuf};

use crate::config;

pub fn env_name(name: &str) -> String {
    let upper: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '_' })
        .collect();
    format!("FORMSLOT_{upper}_BIN")
}

fn runnable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn in_dir(dir: &Path, name: &str) -> Option<PathBuf> {
    let plain = dir.join(name);
    if runnable(&plain) {
        return Some(plain);
    }
    if cfg!(windows) {
        for ext in ["exe", "cmd", "bat"] {
            let candidate = dir.join(format!("{name}.{ext}"));
            if runnable(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

pub fn on_path(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path).filter(|dir| !dir.as_os_str().is_empty()).find_map(|dir| in_dir(&dir, name))
}

#[cfg(unix)]
fn trusted(path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    let me = std::fs::metadata("/proc/self").or_else(|_| std::fs::metadata(config::data_dir())).map(|m| m.uid());
    meta.mode() & 0o022 == 0 && me.is_ok_and(|uid| uid == meta.uid())
}

#[cfg(not(unix))]
fn trusted(_path: &Path) -> bool {
    true
}

fn data_bin() -> PathBuf {
    config::data_dir_path().join("bin")
}

fn in_data_bin(name: &str) -> Option<PathBuf> {
    let dir = data_bin();
    if dir.exists() && !trusted(&dir) {
        return None;
    }
    in_dir(&dir, name).filter(|p| trusted(p))
}

pub fn refused(name: &str) -> Option<PathBuf> {
    let found = in_dir(&data_bin(), name)?;
    in_data_bin(name).is_none().then_some(found)
}

pub fn find(name: &str) -> Option<PathBuf> {
    if let Some(value) = env::var_os(env_name(name)).filter(|v| !v.is_empty()) {
        let given = PathBuf::from(value);
        return runnable(&given).then_some(given);
    }
    in_data_bin(name).or_else(|| on_path(name))
}

pub fn command(name: &str) -> PathBuf {
    find(name).unwrap_or_else(|| PathBuf::from(name))
}

pub fn source(name: &str) -> &'static str {
    if env::var_os(env_name(name)).is_some_and(|v| !v.is_empty()) {
        "environment"
    } else if in_data_bin(name).is_some() {
        "data folder"
    } else if on_path(name).is_some() {
        "PATH"
    } else {
        "not found"
    }
}

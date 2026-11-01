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

pub fn find(name: &str) -> Option<PathBuf> {
    if let Some(value) = env::var_os(env_name(name)).filter(|v| !v.is_empty()) {
        let given = PathBuf::from(value);
        return runnable(&given).then_some(given);
    }
    in_dir(&config::data_dir().join("bin"), name).or_else(|| on_path(name))
}

pub fn command(name: &str) -> PathBuf {
    find(name).unwrap_or_else(|| PathBuf::from(name))
}

pub fn source(name: &str) -> &'static str {
    if env::var_os(env_name(name)).is_some_and(|v| !v.is_empty()) {
        "environment"
    } else if in_dir(&config::data_dir().join("bin"), name).is_some() {
        "data folder"
    } else if on_path(name).is_some() {
        "PATH"
    } else {
        "not found"
    }
}

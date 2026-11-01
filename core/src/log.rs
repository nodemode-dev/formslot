use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::config;

const LIMIT: u64 = 1024 * 1024;

pub fn path() -> PathBuf {
    config::log_file()
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

pub fn record(event: &str, fields: Value) {
    let file = path();
    if let Some(dir) = file.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if fs::metadata(&file).map(|m| m.len() > LIMIT).unwrap_or(false) {
        let _ = fs::rename(&file, file.with_file_name("formslot.1.log"));
    }
    let mut line = json!({ "t": now_ms(), "event": event, "pid": std::process::id() });
    if let (Some(base), Some(extra)) = (line.as_object_mut(), fields.as_object()) {
        for (k, v) in extra {
            base.insert(k.clone(), v.clone());
        }
    }
    crate::redact::value(&mut line);
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    if let Ok(mut out) = options.open(&file) {
        let _ = writeln!(out, "{line}");
    }
}

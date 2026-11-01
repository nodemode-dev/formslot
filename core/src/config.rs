use std::env;
use std::fs;
use std::path::PathBuf;

pub const DEFAULT_PORT: u16 = 5645;

pub fn port() -> u16 {
    env::var("FORMSLOT_PORT")
        .ok()
        .and_then(|v| v.trim().parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT)
}

fn home() -> PathBuf {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn data_dir() -> PathBuf {
    let dir = match env::var_os("FORMSLOT_DATA_DIR") {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => home().join(".formslot"),
    };
    if !dir.exists() {
        let _ = fs::create_dir_all(&dir);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
        }
    }
    dir
}

pub fn corpus_db() -> PathBuf {
    data_dir().join("corpus.sqlite")
}

pub fn log_file() -> PathBuf {
    data_dir().join("formslot.log")
}

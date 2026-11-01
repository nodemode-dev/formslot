use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::{config, Result, VERSION};

pub const FORMAT: &str = "formslot.service";
pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    pub format: String,
    pub format_version: u32,
    pub port: u16,
    pub token: String,
    pub pid: u32,
    pub version: String,
    pub started: String,
}

impl ServiceInfo {
    pub fn new(port: u16, token: String) -> ServiceInfo {
        ServiceInfo {
            format: FORMAT.to_string(),
            format_version: FORMAT_VERSION,
            port,
            token,
            pid: std::process::id(),
            version: VERSION.to_string(),
            started: now_rfc3339(),
        }
    }
}

pub fn info_path() -> PathBuf {
    config::data_dir().join("service.json")
}

pub fn lock_path() -> PathBuf {
    config::data_dir().join("service.lock")
}

fn private_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
}

pub fn acquire_lock() -> Result<Option<File>> {
    let path = lock_path();
    let file = private_options()
        .read(true)
        .truncate(false)
        .open(&path)
        .map_err(|e| format!("the lock file {} could not be opened ({e})", path.display()))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(e)) => Err(format!("the lock file {} could not be locked ({e})", path.display())),
    }
}

pub fn write(info: &ServiceInfo) -> Result<()> {
    let path = info_path();
    let temp = path.with_file_name("service.json.tmp");
    let text = serde_json::to_string_pretty(info).map_err(|e| e.to_string())?;
    {
        let mut out = private_options()
            .truncate(true)
            .open(&temp)
            .map_err(|e| format!("{} could not be written ({e})", temp.display()))?;
        out.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
        out.write_all(b"\n").map_err(|e| e.to_string())?;
        out.sync_all().map_err(|e| e.to_string())?;
    }
    fs::rename(&temp, &path).map_err(|e| format!("{} could not be replaced ({e})", path.display()))
}

pub fn read() -> Result<Option<ServiceInfo>> {
    let path = info_path();
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{} could not be read ({e})", path.display())),
    };
    let info: ServiceInfo = serde_json::from_str(&text).map_err(|e| format!("{} is not a formslot discovery file ({e})", path.display()))?;
    if info.format != FORMAT {
        return Err(format!("{} is not a formslot discovery file", path.display()));
    }
    Ok(Some(info))
}

pub fn remove() {
    let _ = fs::remove_file(info_path());
}

pub fn pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    if cfg!(target_os = "linux") && std::path::Path::new("/proc/self").exists() {
        return std::path::Path::new("/proc").join(pid.to_string()).exists();
    }
    if cfg!(windows) {
        return Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .stderr(Stdio::null())
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
            .unwrap_or(false);
    }
    Command::new("ps")
        .args(["-p", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn new_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the system random source is unavailable");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn now_rfc3339() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let rest = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + if month <= 2 { 1 } else { 0 };
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", rest / 3_600, rest % 3_600 / 60, rest % 60)
}

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use serde_json::{Map, Value, json};

use crate::{Result, config, redact};

const SERVICE: &str = "formslot";
pub const ANTHROPIC: &str = "anthropic";
pub const ANTHROPIC_ENV: &str = "ANTHROPIC_API_KEY";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    System,
    File,
}

impl Place {
    pub fn id(self) -> &'static str {
        match self {
            Place::System => "system",
            Place::File => "file",
        }
    }

    pub fn words(self) -> &'static str {
        match self {
            Place::System => "the system password store",
            Place::File => "a file only you can read",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Status {
    pub saved: bool,
    pub place: Option<Place>,
    pub last4: String,
    pub env: bool,
}

pub fn index_path() -> PathBuf {
    config::data_dir().join("keys.json")
}

pub fn file_path(name: &str) -> PathBuf {
    config::data_dir().join(format!("{name}.key"))
}

fn write_private(path: &PathBuf, text: &str) -> Result<()> {
    let temp = path.with_extension("tmp");
    let _ = fs::remove_file(&temp);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut out = options.open(&temp).map_err(|e| format!("{} could not be written ({e})", temp.display()))?;
    out.write_all(text.as_bytes())
        .and_then(|()| out.sync_all())
        .map_err(|e| format!("{} could not be written ({e})", temp.display()))?;
    drop(out);
    fs::rename(&temp, path).map_err(|e| format!("{} could not be replaced ({e})", path.display()))
}

fn index() -> Map<String, Value> {
    fs::read_to_string(index_path())
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default()
}

fn index_set(name: &str, place: Place, value: &str) -> Result<()> {
    let mut all = index();
    all.insert(name.to_string(), json!({ "where": place.id(), "last4": last4(value) }));
    write_private(&index_path(), &Value::Object(all).to_string())
}

fn index_drop(name: &str) -> Result<()> {
    let mut all = index();
    if all.remove(name).is_none() {
        return Ok(());
    }
    write_private(&index_path(), &Value::Object(all).to_string())
}

fn index_place(name: &str) -> Option<Place> {
    match index().get(name)?.get("where")?.as_str()? {
        "system" => Some(Place::System),
        "file" => Some(Place::File),
        _ => None,
    }
}

fn last4(value: &str) -> String {
    let count = value.chars().count();
    if count < 8 {
        return String::new();
    }
    value.chars().skip(count - 4).collect()
}

fn store_set(name: &str, value: &str) -> bool {
    let Ok(entry) = keyring::Entry::new(SERVICE, name) else {
        return false;
    };
    entry.set_password(value).is_ok() && entry.get_password().is_ok_and(|read| read == value)
}

fn store_get(name: &str) -> (Option<String>, bool) {
    let Ok(entry) = keyring::Entry::new(SERVICE, name) else {
        return (None, false);
    };
    match entry.get_password() {
        Ok(found) if !found.trim().is_empty() => (Some(found.trim().to_string()), true),
        Ok(_) | Err(keyring::Error::NoEntry) => (None, true),
        Err(_) => (None, false),
    }
}

fn store_delete(name: &str) -> bool {
    keyring::Entry::new(SERVICE, name).is_ok_and(|entry| entry.delete_credential().is_ok())
}

fn file_get(name: &str) -> Option<String> {
    let text = fs::read_to_string(file_path(name)).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

pub fn set(name: &str, value: &str) -> Result<Place> {
    let value = value.trim();
    if value.is_empty() {
        return Err("no key was given".to_string());
    }
    if value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("a key is one line with no spaces".to_string());
    }
    redact::remember(value);
    if store_set(name, value) {
        index_set(name, Place::System, value)?;
        let _ = fs::remove_file(file_path(name));
        return Ok(Place::System);
    }
    write_private(&file_path(name), value)?;
    index_set(name, Place::File, value)?;
    Ok(Place::File)
}

pub fn remove(name: &str) -> Result<bool> {
    let listed = index().contains_key(name);
    let in_store = store_delete(name);
    let in_file = fs::remove_file(file_path(name)).is_ok();
    index_drop(name)?;
    Ok(listed || in_store || in_file)
}

pub fn status(name: &str, env: &str) -> Status {
    let entry = index().get(name).cloned();
    let place = index_place(name);
    Status {
        saved: place.is_some(),
        place,
        last4: entry.as_ref().and_then(|e| e.get("last4")).and_then(Value::as_str).unwrap_or("").to_string(),
        env: std::env::var(env).is_ok_and(|v| !v.trim().is_empty()),
    }
}

pub fn get(name: &str, env: &str) -> Result<String> {
    let found = match index_place(name) {
        Some(Place::System) => {
            let (value, answered) = store_get(name);
            if value.is_none() && answered {
                let _ = index_drop(name);
            }
            value
        }
        Some(Place::File) => {
            let value = file_get(name);
            if value.is_none() {
                let _ = index_drop(name);
            }
            value
        }
        None => {
            let (value, _) = store_get(name);
            if let Some(value) = &value {
                let _ = index_set(name, Place::System, value);
            }
            value
        }
    };
    if let Some(value) = found {
        redact::remember(&value);
        return Ok(value);
    }
    match std::env::var(env) {
        Ok(v) if !v.trim().is_empty() => {
            redact::remember(v.trim());
            Ok(v.trim().to_string())
        }
        _ => Err(format!("no key is saved: run `formslot key set`, or set {env}")),
    }
}

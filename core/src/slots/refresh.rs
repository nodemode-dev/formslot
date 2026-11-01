use std::path::Path;

use rusqlite::{OptionalExtension, params};
use serde::Serialize;
use serde_json::{Value, json};

use crate::corpus::{self, Corpus};
use crate::git::Repo;
use crate::slots::{copies, manifest, storage};
use crate::store::Store;

#[derive(Debug, Clone, Serialize)]
pub struct Refreshed {
    pub id: String,
    pub corpus: String,
    pub commit: String,
    pub files: usize,
    pub changed_files: usize,
    pub copies: usize,
    pub content_hash: String,
    pub content_hash_changed: bool,
}

fn find_corpus(store: &Store, key: &str) -> crate::Result<Corpus> {
    let all = corpus::list(store)?;
    all.iter()
        .find(|c| c.name == key)
        .or_else(|| all.iter().find(|c| c.id.to_string() == key))
        .cloned()
        .ok_or_else(|| format!("no corpus named {key}"))
}

pub fn refresh(store: &mut Store, id: &str, corpus_key: &str) -> crate::Result<Refreshed> {
    let row = storage::must_get(store.conn(), id)?;
    let found = find_corpus(store, corpus_key)?;
    let origin_repo = manifest::text_at(&row.manifest, &["origin", "repo"]).unwrap_or("");
    if origin_repo != found.name {
        return Err(format!(
            "{id} has its origin in repo '{origin_repo}', not in corpus '{}'; refresh against the origin corpus",
            found.name
        ));
    }
    let head = found
        .head
        .clone()
        .ok_or_else(|| format!("corpus {} has no indexed commit yet", found.name))?;
    let repo = Repo::open(Path::new(&found.path))?;
    let mut reader = repo.blob_reader()?;
    let mut value = row.manifest.clone();
    let before = manifest::files_of(&value);
    let mut changed = 0usize;
    let mut paths = Vec::new();
    let files = value
        .get_mut("files")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "the manifest has no files".to_string())?;
    for item in files.iter_mut() {
        let path = item
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| "a file has no path".to_string())?
            .to_string();
        let oid: Option<String> = store
            .conn()
            .query_row(
                "SELECT oid FROM file WHERE corpus_id = ?1 AND path = ?2",
                params![found.id, path],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| format!("slots: {e}"))?;
        let oid = oid.ok_or_else(|| {
            format!("{path} is not in corpus {} at {}", found.name, &head[..head.len().min(12)])
        })?;
        let data = reader
            .read(&oid)?
            .ok_or_else(|| format!("the blob for {path} is missing from {}", found.path))?;
        let (sha, lines) = manifest::hash_text(&data);
        let old = before.iter().find(|f| f.path == path);
        if old.is_none_or(|f| f.sha256 != sha || f.lines != Some(lines)) {
            changed += 1;
        }
        if let Some(object) = item.as_object_mut() {
            object.insert("sha256".to_string(), json!(sha));
            object.insert("lines".to_string(), json!(lines));
        }
        paths.push(path);
    }
    let pairs = manifest::hashed_pairs(&value).ok_or_else(|| "hashing left a file without sha256".to_string())?;
    let hash = manifest::content_hash(&pairs);
    let hash_changed = manifest::text_at(&value, &["contentHash"]) != Some(hash.as_str());
    let derived = copies::derive(store.conn(), found.id, &paths)?;
    if let Some(object) = value.as_object_mut() {
        object.insert("contentHash".to_string(), json!(hash));
        object.insert("copies".to_string(), Value::Array(derived.clone()));
        if let Some(origin) = object.get_mut("origin").and_then(Value::as_object_mut) {
            origin.insert("commit".to_string(), json!(head));
        }
    }
    let note = format!(
        "corpus {} at {}: {} files hashed, {} changed, {} copies",
        found.name,
        &head[..head.len().min(12)],
        paths.len(),
        changed,
        derived.len()
    );
    storage::save(store, &value, "refresh", "cli", &note)?;
    Ok(Refreshed {
        id: id.to_string(),
        corpus: found.name,
        commit: head,
        files: paths.len(),
        changed_files: changed,
        copies: derived.len(),
        content_hash: hash,
        content_hash_changed: hash_changed,
    })
}

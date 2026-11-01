use std::path::PathBuf;
use std::sync::Mutex;

use serde_json::{Value, json};

use crate::http::{Reply, Request};
use crate::store::Store;
use crate::{CONTRACT, VERSION, config, corpus};

static INDEXING: Mutex<()> = Mutex::new(());

pub fn route(req: &Request) -> Reply {
    let parts: Vec<&str> = req.path.trim_end_matches('/').split('/').skip(1).collect();
    match (req.method.as_str(), parts.as_slice()) {
        ("GET", ["api", "health"]) => Reply::ok(&json!({ "ok": true, "version": VERSION, "contract": CONTRACT })),
        ("GET", ["api", "corpus"]) => with_store(|store| Ok(json!({ "corpora": corpus::list(store)? }))),
        ("POST", ["api", "corpus"]) => {
            let body = match parse(&req.body) {
                Ok(body) => body,
                Err(reply) => return reply,
            };
            let Some(path) = body.get("path").and_then(Value::as_str) else {
                return Reply::error(400, "bad_request", "The body needs a \"path\".");
            };
            let rev = body.get("ref").and_then(Value::as_str).unwrap_or("HEAD").to_string();
            let path = PathBuf::from(path);
            if !path.is_absolute() {
                return Reply::error(400, "bad_request", "The \"path\" must be absolute.");
            }
            indexing(|store| {
                let (found, run) = corpus::add(store, &path, &rev)?;
                Ok(json!({ "corpus": found, "index": run }))
            })
        }
        ("POST", ["api", "corpus", id, "reindex"]) => {
            let body = match parse(&req.body) {
                Ok(body) => body,
                Err(reply) => return reply,
            };
            let rev = body.get("ref").and_then(Value::as_str).unwrap_or("HEAD").to_string();
            let id = id.to_string();
            indexing(|store| {
                let (found, run) = corpus::reindex(store, &id, &rev)?;
                Ok(json!({ "corpus": found, "index": run }))
            })
        }
        ("GET", ["api", "corpus", id, "stats"]) => {
            with_store(|store| serde_json::to_value(corpus::stats(store, id)?).map_err(|e| e.to_string()))
        }
        _ => Reply::error(404, "not_found", "No such route."),
    }
}

fn parse(body: &[u8]) -> Result<Value, Reply> {
    if body.is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_slice(body).map_err(|_| Reply::error(400, "bad_request", "The body is not JSON."))
}

fn with_store(work: impl FnOnce(&Store) -> crate::Result<Value>) -> Reply {
    match Store::open(&config::corpus_db()).and_then(|store| work(&store)) {
        Ok(value) => Reply::ok(&value),
        Err(message) => Reply::error(422, "failed", &message),
    }
}

fn indexing(work: impl FnOnce(&mut Store) -> crate::Result<Value>) -> Reply {
    let _turn = INDEXING.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    match Store::open(&config::corpus_db()).and_then(|mut store| work(&mut store)) {
        Ok(value) => Reply::ok(&value),
        Err(message) => Reply::error(422, "failed", &message),
    }
}

use std::path::PathBuf;
use std::sync::Mutex;

use serde_json::{Value, json};

use crate::http::{Reply, Request};
use crate::store::Store;
use crate::{CONTRACT, VERSION, config, corpus, lifecycle, slots};

pub(crate) static INDEXING: Mutex<()> = Mutex::new(());

fn allowed(parts: &[&str]) -> Option<&'static str> {
    match parts {
        ["api", "health"] | ["api", "slots"] | ["api", "slots", _] | ["api", "slots", _, "source"] | ["api", "corpus", _, "stats"] => Some("GET"),
        ["api", "corpus"] => Some("GET, POST"),
        ["api", "corpus", _, "reindex"] | ["api", "shutdown"] => Some("POST"),
        ["api", "lease"] => Some("GET, POST"),
        ["api", "lease", _] => Some("DELETE"),
        ["mcp"] => Some("POST"),
        _ => None,
    }
}

fn corpus_exists(key: &str) -> crate::Result<bool> {
    let store = Store::open(&config::corpus_db())?;
    Ok(corpus::list(&store)?.iter().any(|c| c.name == key || c.id.to_string() == key))
}

fn missing_corpus(key: &str) -> Option<Reply> {
    match corpus_exists(key) {
        Ok(true) => None,
        Ok(false) => Some(Reply::error(404, "not_found", "No such corpus.")),
        Err(message) => Some(Reply::error(422, "failed", &message)),
    }
}

fn lease_post(body: &[u8]) -> Reply {
    let body = match parse(body) {
        Ok(body) => body,
        Err(reply) => return reply,
    };
    if let Some(id) = body.get("lease").and_then(Value::as_str) {
        return match lifecycle::renew(id) {
            Some(view) => Reply::ok(&view),
            None => Reply::error(404, "lease_expired", "No such lease; it expired or was released. Open a new one."),
        };
    }
    let Some(client) = body.get("client").and_then(Value::as_str).filter(|c| lifecycle::client_ok(c)) else {
        return Reply::error(400, "bad_request", "The body needs a \"client\" name of 1 to 64 characters, or the \"lease\" to renew.");
    };
    match lifecycle::open(client) {
        Ok(view) => Reply::json(201, &view),
        Err(message) => Reply::error(429, "too_many_leases", &message),
    }
}

pub fn route(req: &Request) -> Reply {
    let probe = req.method == "GET" && matches!(req.path.trim_end_matches('/'), "/api/health" | "/api/lease");
    let _busy = (!probe).then(lifecycle::busy);
    dispatch(req)
}

fn dispatch(req: &Request) -> Reply {
    let parts: Vec<&str> = req.path.trim_end_matches('/').split('/').skip(1).collect();
    match (req.method.as_str(), parts.as_slice()) {
        ("POST", ["api", "lease"]) => lease_post(&req.body),
        ("GET", ["api", "lease"]) => Reply::ok(&lifecycle::snapshot()),
        ("DELETE", ["api", "lease", id]) => {
            if lifecycle::release(id) {
                Reply::ok(&json!({ "released": true }))
            } else {
                Reply::error(404, "lease_expired", "No such lease; it expired or was released.")
            }
        }
        ("POST", ["api", "shutdown"]) => {
            lifecycle::request_stop();
            Reply::json(202, &json!({ "stopping": true }))
        }
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
            if let Some(reply) = missing_corpus(id) {
                return reply;
            }
            let rev = body.get("ref").and_then(Value::as_str).unwrap_or("HEAD").to_string();
            let id = id.to_string();
            indexing(|store| {
                let (found, run) = corpus::reindex(store, &id, &rev)?;
                Ok(json!({ "corpus": found, "index": run }))
            })
        }
        ("GET", ["api", "corpus", id, "stats"]) => {
            if let Some(reply) = missing_corpus(id) {
                return reply;
            }
            with_store(|store| serde_json::to_value(corpus::stats(store, id)?).map_err(|e| e.to_string()))
        }
        ("GET", ["api", "slots"]) => slots::web::list(&req.query),
        ("GET", ["api", "slots", id]) => slots::web::get(id),
        ("GET", ["api", "slots", id, "source"]) => slots::web::source(id, &req.query),
        ("POST", ["mcp"]) => crate::mcp::http::post(req),
        (_, ["mcp"]) => Reply::error(405, "method_not_allowed", "POST JSON-RPC messages to /mcp.").with_header("Allow", "POST"),
        (_, other) => match allowed(other) {
            Some(methods) => Reply::error(405, "method_not_allowed", "That method is not served on this path.").with_header("Allow", methods),
            None => Reply::error(404, "not_found", "No such route."),
        },
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

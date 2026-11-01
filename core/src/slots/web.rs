use rusqlite::Connection;
use serde_json::{Value, json};

use crate::http::Reply;
use crate::slots::{manifest, storage};
use crate::store::Store;
use crate::config;

fn summary(row: &storage::SlotRow) -> Value {
    let m = &row.manifest;
    json!({
        "id": row.id,
        "status": row.status,
        "version": row.version,
        "kind": manifest::text_at(m, &["kind"]).unwrap_or(""),
        "title": manifest::text_at(m, &["title"]).unwrap_or(""),
        "summary": manifest::text_at(m, &["summary"]).unwrap_or(""),
        "contentHash": row.content_hash,
        "files": m.get("files").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
        "copies": m.get("copies").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
        "updatedAt": row.updated_at,
    })
}

fn open() -> Result<Store, String> {
    Store::open(&config::corpus_db())
}

fn status_filter(query: &str) -> Option<String> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(k, _)| *k == "status")
        .map(|(_, v)| v.to_string())
}

pub fn list_value(conn: &Connection, status: Option<&str>) -> crate::Result<Value> {
    let rows = storage::list(conn, status)?;
    Ok(json!({ "slots": rows.iter().map(summary).collect::<Vec<_>>() }))
}

pub fn get_value(conn: &Connection, id: &str) -> crate::Result<Option<Value>> {
    let Some(row) = storage::get(conn, id)? else {
        return Ok(None);
    };
    let events = storage::events(conn, id)?;
    Ok(Some(json!({
        "slot": row.manifest,
        "status": row.status,
        "updatedAt": row.updated_at,
        "events": events,
    })))
}

pub fn list(query: &str) -> Reply {
    let status = status_filter(query);
    if let Some(s) = &status {
        if !manifest::STATUSES.contains(&s.as_str()) {
            return Reply::error(400, "bad_request", "The status is not one of proposed, canonical, deprecated, declined, superseded.");
        }
    }
    match open().and_then(|store| list_value(store.conn(), status.as_deref())) {
        Ok(value) => Reply::ok(&value),
        Err(message) => Reply::error(422, "failed", &message),
    }
}

fn file_filter(query: &str) -> Option<String> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(k, _)| *k == "file")
        .map(|(_, v)| percent_decode(v))
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() && bytes[i + 1].is_ascii_hexdigit() && bytes[i + 2].is_ascii_hexdigit() {
            let pair = &text[i + 1..i + 3];
            out.push(u8::from_str_radix(pair, 16).unwrap_or(b'%'));
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn source(id: &str, query: &str) -> Reply {
    if !manifest::is_id(id) {
        return Reply::error(404, "not_found", "No such slot.");
    }
    let file = file_filter(query);
    match open().and_then(|store| super::source::read(store.conn(), id, file.as_deref(), super::source::Caps::STANDARD)) {
        Ok(Some(value)) => Reply::ok(&value),
        Ok(None) => Reply::error(404, "not_found", "No such slot."),
        Err(message) => Reply::error(422, "failed", &message),
    }
}

pub fn get(id: &str) -> Reply {
    if !manifest::is_id(id) {
        return Reply::error(404, "not_found", "No such slot.");
    }
    match open().and_then(|store| get_value(store.conn(), id)) {
        Ok(Some(value)) => Reply::ok(&value),
        Ok(None) => Reply::error(404, "not_found", "No such slot."),
        Err(message) => Reply::error(422, "failed", &message),
    }
}

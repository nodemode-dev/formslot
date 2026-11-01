use rusqlite::params;
use serde_json::{Value, json};

use super::page::{self, CAP_TOKENS};
use super::tools::open;
use super::Rpc;
use crate::memory;
use crate::redact;
use crate::search::{self, map};
use crate::slots::{manifest, storage, web};

const PAGE: usize = 50;
const SLOT: &str = "slot://slots/";
const MEMORY: &str = "slot://memory/";
const MAP: &str = "slot://map/";

pub fn templates() -> Value {
    json!({ "resourceTemplates": [
        {
            "uriTemplate": "slot://slots/{id}",
            "name": "slot",
            "title": "Slot manifest",
            "description": "One slot's manifest as JSON: files, hashes, API, copies and status. Ids come from slot_find or resources/list.",
            "mimeType": "application/json"
        },
        {
            "uriTemplate": "slot://memory/{id}",
            "name": "memory-record",
            "title": "Memory record",
            "description": "One memory record (an owner rule, a decline, a decision, a HASHOUT entry) as JSON, with its validity dates.",
            "mimeType": "application/json"
        },
        {
            "uriTemplate": "slot://map/{corpus}",
            "name": "formmap",
            "title": "FORMMAP",
            "description": "The ranked repo map of one corpus with owner rules and shared slots, under 150 lines.",
            "mimeType": "text/markdown"
        }
    ] })
}

pub fn list(cursor: Option<&str>) -> Result<Value, Rpc> {
    let start = match cursor {
        Some(c) => page::decode(c, "resources", "").map_err(|m| Rpc::new(-32602, m))?,
        None => 0,
    };
    let store = open().map_err(Rpc::internal)?;
    let conn = store.conn();
    let slots = storage::list(conn, None).map_err(Rpc::internal)?;
    let maps = search::targets(conn).map_err(Rpc::internal)?;
    let total = slots.len() + maps.len();
    if start > total {
        return Err(Rpc::new(-32602, "The cursor is past the end of the list."));
    }
    let mut out = Vec::new();
    for i in start..total.min(start + PAGE) {
        if i < slots.len() {
            let s = &slots[i];
            out.push(json!({
                "uri": format!("{SLOT}{}", s.id),
                "name": s.id,
                "title": manifest::text_at(&s.manifest, &["title"]).unwrap_or(""),
                "description": format!("{} · {}", s.status, manifest::text_at(&s.manifest, &["summary"]).unwrap_or("")),
                "mimeType": "application/json"
            }));
        } else {
            let m = &maps[i - slots.len()];
            out.push(json!({
                "uri": format!("{MAP}{}", m.name),
                "name": format!("map-{}", m.name),
                "title": format!("FORMMAP of {}", m.name),
                "description": "The ranked repo map with owner rules and shared slots.",
                "mimeType": "text/markdown"
            }));
        }
    }
    let mut result = json!({ "resources": out });
    if start + PAGE < total {
        result["nextCursor"] = json!(page::encode("resources", "", start + PAGE));
    }
    Ok(result)
}

fn cap(text: String) -> String {
    let (clean, _) = redact::text(&text);
    let room = (CAP_TOKENS - 100) * 36 / 10;
    if clean.len() <= room {
        return clean;
    }
    let mut cut = room;
    while cut > 0 && !clean.is_char_boundary(cut) {
        cut -= 1;
    }
    if let Some(nl) = clean[..cut].rfind('\n') {
        cut = nl;
    }
    format!("{}\n[truncated by formslot: held under {CAP_TOKENS} tokens]", &clean[..cut])
}

fn missing(uri: &str) -> Rpc {
    Rpc { code: -32002, message: "Resource not found".to_string(), data: Some(json!({ "uri": uri })) }
}

pub fn read(uri: &str) -> Result<Value, Rpc> {
    let store = open().map_err(Rpc::internal)?;
    let conn = store.conn();
    if let Some(id) = uri.strip_prefix(SLOT) {
        if !manifest::is_id(id) {
            return Err(missing(uri));
        }
        let value = web::get_value(conn, id).map_err(Rpc::internal)?.ok_or_else(|| missing(uri))?;
        let text = serde_json::to_string_pretty(&value).unwrap_or_default();
        return Ok(contents(uri, "application/json", text));
    }
    if let Some(id) = uri.strip_prefix(MEMORY) {
        let id: i64 = id.parse().map_err(|_| missing(uri))?;
        let query = format!("SELECT {} FROM memory_record WHERE id = ?1", memory::COLUMNS);
        let record = conn.query_row(&query, params![id], memory::from_row).map_err(|_| missing(uri))?;
        let text = serde_json::to_string_pretty(&record).unwrap_or_default();
        return Ok(contents(uri, "application/json", text));
    }
    if let Some(name) = uri.strip_prefix(MAP) {
        let target = search::resolve_one(conn, Some(name)).map_err(|_| missing(uri))?;
        search::ensure(conn, std::slice::from_ref(&target)).map_err(Rpc::internal)?;
        let text = map::formmap(conn, &target, map::FORMMAP_TOKENS).map_err(Rpc::internal)?;
        return Ok(contents(uri, "text/markdown", text));
    }
    Err(missing(uri))
}

fn contents(uri: &str, mime: &str, text: String) -> Value {
    json!({ "contents": [{ "uri": uri, "mimeType": mime, "text": cap(text) }] })
}

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde_json::Value;

use crate::corpus::now_iso;
use crate::slots::manifest;
use crate::store::Store;

#[derive(Debug, Clone)]
pub struct SlotRow {
    pub id: String,
    pub status: String,
    pub version: String,
    pub manifest: Value,
    pub content_hash: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Event {
    pub id: i64,
    pub slot_id: String,
    pub at: String,
    pub action: String,
    pub actor: String,
    pub note: String,
}

fn sql(e: rusqlite::Error) -> String {
    format!("slots: {e}")
}

const COLUMNS: &str = "id, status, version, manifest, content_hash, updated_at";

fn row_of(r: &rusqlite::Row) -> rusqlite::Result<SlotRow> {
    let text: String = r.get(3)?;
    Ok(SlotRow {
        id: r.get(0)?,
        status: r.get(1)?,
        version: r.get(2)?,
        manifest: serde_json::from_str(&text).unwrap_or(Value::Null),
        content_hash: r.get(4)?,
        updated_at: r.get(5)?,
    })
}

pub fn get(conn: &Connection, id: &str) -> crate::Result<Option<SlotRow>> {
    let query = format!("SELECT {COLUMNS} FROM slot WHERE id = ?1");
    conn.query_row(&query, params![id], row_of).optional().map_err(sql)
}

pub fn list(conn: &Connection, status: Option<&str>) -> crate::Result<Vec<SlotRow>> {
    let query = format!("SELECT {COLUMNS} FROM slot WHERE (?1 IS NULL OR status = ?1) ORDER BY id");
    let mut stmt = conn.prepare(&query).map_err(sql)?;
    let rows = stmt.query_map(params![status], row_of).map_err(sql)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(sql)?);
    }
    Ok(out)
}

pub fn events(conn: &Connection, id: &str) -> crate::Result<Vec<Event>> {
    let mut stmt = conn
        .prepare("SELECT id, slot_id, at, action, actor, note FROM slot_event WHERE slot_id = ?1 ORDER BY id")
        .map_err(sql)?;
    let rows = stmt
        .query_map(params![id], |r| {
            Ok(Event {
                id: r.get(0)?,
                slot_id: r.get(1)?,
                at: r.get(2)?,
                action: r.get(3)?,
                actor: r.get(4)?,
                note: r.get(5)?,
            })
        })
        .map_err(sql)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(sql)?);
    }
    Ok(out)
}

pub fn log_event(conn: &Connection, id: &str, action: &str, actor: &str, note: &str) -> crate::Result<()> {
    conn.execute(
        "INSERT INTO slot_event (slot_id, at, action, actor, note) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, now_iso(), action, actor, note],
    )
    .map_err(sql)?;
    Ok(())
}

pub fn save(store: &mut Store, value: &Value, action: &str, actor: &str, note: &str) -> crate::Result<()> {
    manifest::check(value)?;
    let id = manifest::id_of(value).to_string();
    let status = manifest::status_of(value).to_string();
    let version = manifest::text_at(value, &["version"]).unwrap_or("").to_string();
    let hash = manifest::text_at(value, &["contentHash"]).unwrap_or("").to_string();
    let text = serde_json::to_string(value).map_err(|e| format!("slots: {e}"))?;
    let now = now_iso();
    let tx = store.conn_mut().transaction().map_err(sql)?;
    tx.execute(
        "INSERT INTO slot (id, status, version, manifest, content_hash, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
         ON CONFLICT(id) DO UPDATE SET status = excluded.status, version = excluded.version, \
         manifest = excluded.manifest, content_hash = excluded.content_hash, updated_at = excluded.updated_at",
        params![id, status, version, text, hash, now],
    )
    .map_err(sql)?;
    tx.execute("UPDATE slot_file SET current = 0 WHERE slot_id = ?1", params![id])
        .map_err(sql)?;
    for file in manifest::files_of(value) {
        tx.execute(
            "INSERT INTO slot_file (slot_id, path, sha256, current) VALUES (?1, ?2, ?3, 1) \
             ON CONFLICT(slot_id, path) DO UPDATE SET sha256 = excluded.sha256, current = 1",
            params![id, file.path, file.sha256],
        )
        .map_err(sql)?;
    }
    tx.execute(
        "INSERT INTO slot_event (slot_id, at, action, actor, note) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, now, action, actor, note],
    )
    .map_err(sql)?;
    tx.commit().map_err(sql)?;
    Ok(())
}

pub fn must_get(conn: &Connection, id: &str) -> crate::Result<SlotRow> {
    get(conn, id)?.ok_or_else(|| format!("no slot with id {id}"))
}

pub fn confirm(store: &mut Store, id: &str) -> crate::Result<SlotRow> {
    let row = must_get(store.conn(), id)?;
    match row.status.as_str() {
        "canonical" => return Err(format!("{id} is already canonical")),
        "declined" | "superseded" => {
            return Err(format!(
                "{id} is {}; a {} record stays as it is, propose a new slot id instead",
                row.status, row.status
            ));
        }
        _ => {}
    }
    let mut value = row.manifest.clone();
    set_status(&mut value, "canonical", None);
    save(store, &value, "confirm", "owner", &format!("{} -> canonical", row.status))?;
    must_get(store.conn(), id)
}

pub fn decline(store: &mut Store, id: &str, reason: &str) -> crate::Result<SlotRow> {
    let row = must_get(store.conn(), id)?;
    if row.status == "declined" {
        return Err(format!("{id} is already declined"));
    }
    if row.status == "superseded" {
        return Err(format!("{id} is superseded; a superseded record stays as it is"));
    }
    let mut value = row.manifest.clone();
    set_status(&mut value, "declined", Some(reason));
    save(store, &value, "decline", "owner", &format!("{} -> declined: {reason}", row.status))?;
    must_get(store.conn(), id)
}

fn set_status(value: &mut Value, status: &str, reason: Option<&str>) {
    if let Some(object) = value.as_object_mut() {
        object.insert("status".to_string(), Value::String(status.to_string()));
        match reason {
            Some(text) => {
                object.insert("statusReason".to_string(), Value::String(text.to_string()));
            }
            None => {
                if status == "canonical" {
                    object.remove("statusReason");
                }
            }
        }
    }
}

pub fn propose(store: &mut Store, value: &Value) -> crate::Result<String> {
    manifest::check(value)?;
    if manifest::status_of(value) != "proposed" {
        return Err(format!(
            "a proposal is stored as proposed; the file says '{}'. Only the owner confirms",
            manifest::status_of(value)
        ));
    }
    let id = manifest::id_of(value).to_string();
    match get(store.conn(), &id)? {
        None => {
            save(store, value, "propose", "agent", "new proposal")?;
            Ok("proposed".to_string())
        }
        Some(row) if row.status == "proposed" => {
            save(store, value, "propose", "agent", "proposal replaced")?;
            Ok("replaced".to_string())
        }
        Some(row) => Err(format!(
            "{id} already exists as {}; a record that has been decided is not overwritten, propose a new id",
            row.status
        )),
    }
}

use rusqlite::params;
use serde_json::{Value, json};

use super::tools::open;
use super::Rpc;
use crate::slots::storage;
use crate::search;

const MAX: usize = 100;

fn done(values: Vec<String>) -> Value {
    let total = values.len();
    let shown: Vec<String> = values.into_iter().take(MAX).collect();
    json!({ "completion": { "values": shown, "total": total, "hasMore": total > MAX } })
}

fn like_escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

fn corpora(prefix: &str) -> Result<Vec<String>, Rpc> {
    let store = open().map_err(Rpc::internal)?;
    let all = search::targets(store.conn()).map_err(Rpc::internal)?;
    Ok(all.into_iter().map(|t| t.name).filter(|n| n.starts_with(prefix)).collect())
}

pub fn complete(params_value: &Value) -> Result<Value, Rpc> {
    let reference = &params_value["ref"];
    let argument = &params_value["argument"];
    let name = argument["name"].as_str().unwrap_or("");
    let prefix = argument["value"].as_str().unwrap_or("");
    match reference["type"].as_str().unwrap_or("") {
        "ref/resource" => {
            let uri = reference["uri"].as_str().unwrap_or("");
            match (uri, name) {
                ("slot://slots/{id}", "id") => {
                    let store = open().map_err(Rpc::internal)?;
                    let rows = storage::list(store.conn(), None).map_err(Rpc::internal)?;
                    Ok(done(rows.into_iter().map(|r| r.id).filter(|id| id.starts_with(prefix)).collect()))
                }
                ("slot://memory/{id}", "id") => {
                    let store = open().map_err(Rpc::internal)?;
                    let mut stmt = store
                        .conn()
                        .prepare("SELECT CAST(id AS TEXT) FROM memory_record WHERE CAST(id AS TEXT) LIKE ?1 || '%' ESCAPE '\\' ORDER BY id LIMIT 101")
                        .map_err(|e| Rpc::internal(e.to_string()))?;
                    let rows = stmt
                        .query_map(params![like_escape(prefix)], |r| r.get::<_, String>(0))
                        .map_err(|e| Rpc::internal(e.to_string()))?;
                    Ok(done(rows.filter_map(Result::ok).collect()))
                }
                ("slot://map/{corpus}", "corpus") => Ok(done(corpora(prefix)?)),
                _ => Ok(done(Vec::new())),
            }
        }
        "ref/prompt" => match (reference["name"].as_str().unwrap_or(""), name) {
            ("pack-for-task" | "conventions", "corpus") => Ok(done(corpora(prefix)?)),
            _ => Ok(done(Vec::new())),
        },
        _ => Err(Rpc::new(-32602, "The completion ref must be ref/resource or ref/prompt.")),
    }
}

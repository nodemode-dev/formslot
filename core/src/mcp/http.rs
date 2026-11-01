use serde_json::Value;

use super::answer_text;
use crate::discovery;
use crate::http::Reply;

fn starts_session(text: &str) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return false;
    };
    let one = |m: &Value| m.get("method").and_then(Value::as_str) == Some("initialize") && m.get("id").is_some();
    match &value {
        Value::Array(batch) => batch.iter().any(one),
        other => one(other),
    }
}

pub fn post(body: &[u8]) -> Reply {
    let Ok(text) = std::str::from_utf8(body) else {
        return Reply::json(400, &serde_json::json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": "Parse error" } }));
    };
    let Some(answer) = answer_text(text, "http") else {
        return Reply { status: 202, body: Vec::new(), headers: Vec::new() };
    };
    let parse_error = answer.get("error").and_then(|e| e.get("code")).and_then(Value::as_i64) == Some(-32700);
    let mut reply = Reply::json(if parse_error { 400 } else { 200 }, &answer);
    if starts_session(text) && answer.get("result").is_some() {
        reply = reply.with_header("Mcp-Session-Id", &discovery::new_token()[..32]);
    }
    reply
}

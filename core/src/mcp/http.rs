use serde_json::{Value, json};

use super::{HEADER_MISMATCH, LEGACY, META_VERSION, METHOD_NOT_FOUND, MODERN, Rpc, UNSUPPORTED_VERSION, answer, answer_text, meta_version, unsupported};
use crate::discovery;
use crate::http::{Reply, Request};

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

fn refusal(id: &Value, error: Rpc) -> Reply {
    let mut body = json!({ "code": error.code, "message": error.message });
    if let Some(data) = error.data {
        body["data"] = data;
    }
    Reply::json(400, &json!({ "jsonrpc": "2.0", "id": id, "error": body }))
}

fn mismatch(id: &Value, message: String) -> Reply {
    refusal(id, Rpc::new(HEADER_MISMATCH, format!("Header mismatch: {message}")))
}

fn unbase64(text: &str) -> Option<String> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut bytes = Vec::new();
    let mut bits = 0u32;
    let mut held = 0u32;
    for c in text.bytes().filter(|c| *c != b'=') {
        let value = TABLE.iter().position(|t| *t == c)? as u32;
        bits = (bits << 6) | value;
        held += 6;
        if held >= 8 {
            held -= 8;
            bytes.push((bits >> held) as u8);
            bits &= (1 << held) - 1;
        }
    }
    String::from_utf8(bytes).ok()
}

fn header_name(value: &str) -> Option<String> {
    match value.strip_prefix("=?base64?").and_then(|rest| rest.strip_suffix("?=")) {
        Some(encoded) => unbase64(encoded),
        None => Some(value.to_string()),
    }
}

fn modern_headers(message: &Value, req: &Request) -> Result<(), Reply> {
    let id = message.get("id").cloned().unwrap_or(Value::Null);
    let header = req.headers.get("mcp-protocol-version").map(String::as_str);
    let meta = meta_version(message);
    let (Some(header), Some(meta)) = (header, meta) else {
        return Err(mismatch(&id, format!("a request under protocol {MODERN} carries its version both in the MCP-Protocol-Version header and in _meta[\"{META_VERSION}\"]")));
    };
    if header != meta {
        return Err(mismatch(&id, format!("MCP-Protocol-Version header value '{header}' does not match body value '{meta}'")));
    }
    if meta != MODERN {
        return Err(refusal(&id, unsupported(meta)));
    }
    let method = message.get("method").and_then(Value::as_str).unwrap_or_default();
    match req.headers.get("mcp-method") {
        Some(given) if given == method => {}
        Some(given) => return Err(mismatch(&id, format!("Mcp-Method header value '{given}' does not match body value '{method}'"))),
        None => return Err(mismatch(&id, "the Mcp-Method header is missing".to_string())),
    }
    let field = match method {
        "tools/call" | "prompts/get" => "name",
        "resources/read" => "uri",
        _ => return Ok(()),
    };
    let named = message.get("params").and_then(|p| p.get(field)).and_then(Value::as_str).unwrap_or_default();
    match req.headers.get("mcp-name").map(|given| header_name(given)) {
        Some(Some(given)) if given == named => Ok(()),
        Some(Some(given)) => Err(mismatch(&id, format!("Mcp-Name header value '{given}' does not match body value '{named}'"))),
        Some(None) => Err(mismatch(&id, "the Mcp-Name header is not valid Base64".to_string())),
        None => Err(mismatch(&id, "the Mcp-Name header is missing".to_string())),
    }
}

fn status_of(answer: &Value) -> u16 {
    match answer.get("error").and_then(|e| e.get("code")).and_then(Value::as_i64) {
        Some(METHOD_NOT_FOUND) => 404,
        Some(UNSUPPORTED_VERSION) | Some(-32700) => 400,
        _ => 200,
    }
}

pub fn post(req: &Request) -> Reply {
    let parse_error = || Reply::json(400, &json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": "Parse error" } }));
    let Ok(text) = std::str::from_utf8(&req.body) else {
        return parse_error();
    };
    let Ok(message) = serde_json::from_str::<Value>(text) else {
        return parse_error();
    };
    let header = req.headers.get("mcp-protocol-version").map(String::as_str);
    let opens = message.get("method").and_then(Value::as_str) == Some("initialize");
    let header = header.filter(|_| !opens);
    let stateless = message.is_object() && !opens && (meta_version(&message).is_some() || header == Some(MODERN));
    if stateless {
        if let Err(reply) = modern_headers(&message, req) {
            return reply;
        }
        return match answer(&message, "http") {
            Some(answer) => Reply::json(status_of(&answer), &answer),
            None => Reply { status: 202, body: Vec::new(), headers: Vec::new() },
        };
    }
    if let Some(given) = header.filter(|given| !LEGACY.contains(given)) {
        let id = message.get("id").cloned().unwrap_or(Value::Null);
        return refusal(&id, unsupported(given));
    }
    let Some(answer) = answer_text(text, "http") else {
        return Reply { status: 202, body: Vec::new(), headers: Vec::new() };
    };
    let mut reply = Reply::json(200, &answer);
    if starts_session(text) && answer.get("result").is_some() {
        reply = reply.with_header("Mcp-Session-Id", &discovery::new_token()[..32]);
    }
    reply
}

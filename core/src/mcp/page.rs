use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::redact;
use crate::search::tokens;

pub const CAP_TOKENS: usize = 25_000;
pub const PAGE_TOKENS: usize = 20_000;

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn print(parts: &[&str]) -> String {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update(part.as_bytes());
        digest.update([0u8]);
    }
    hex(&digest.finalize()[..6])
}

pub fn encode(kind: &str, print: &str, offset: usize) -> String {
    hex(format!("{kind}|{print}|{offset}").as_bytes())
}

pub fn decode(cursor: &str, kind: &str, print: &str) -> Result<usize, String> {
    let wrong = || "The cursor is not valid for this call. Call again without a cursor.".to_string();
    if cursor.len() % 2 != 0 || !cursor.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(wrong());
    }
    let bytes: Vec<u8> = (0..cursor.len())
        .step_by(2)
        .filter_map(|i| u8::from_str_radix(&cursor[i..i + 2], 16).ok())
        .collect();
    let text = String::from_utf8(bytes).map_err(|_| wrong())?;
    let mut parts = text.split('|');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(k), Some(p), Some(o), None) if k == kind && p == print => o.parse::<usize>().map_err(|_| wrong()),
        _ => Err(wrong()),
    }
}

pub struct Item {
    pub text: String,
    pub data: Value,
}

pub struct Listing<'a> {
    pub kind: &'a str,
    pub print: String,
    pub cursor: Option<&'a str>,
    pub size: usize,
    pub noun: &'a str,
    pub header: String,
    pub key: &'a str,
    pub fields: Value,
}

pub fn estimate(text: &str) -> usize {
    tokens::code_text(text)
}

pub fn text_result(blocks: Vec<String>, structured: Option<Value>) -> Value {
    let mut blocks: Vec<String> = blocks.iter().map(|b| redact::text(b).0).collect();
    let mut structured = structured;
    if let Some(value) = structured.as_mut() {
        redact::value(value);
    }
    let structured_tokens = structured.as_ref().map(|v| estimate(&v.to_string())).unwrap_or(0);
    if structured_tokens > CAP_TOKENS / 2 {
        structured = Some(json!({ "truncated": true }));
    }
    let structured_tokens = structured.as_ref().map(|v| estimate(&v.to_string())).unwrap_or(0);
    let room_bytes = (CAP_TOKENS.saturating_sub(structured_tokens + 200) * 36) / 10;
    let total: usize = blocks.iter().map(String::len).sum();
    if total > room_bytes {
        let mut left = room_bytes;
        for block in blocks.iter_mut() {
            if block.len() <= left {
                left -= block.len();
                continue;
            }
            let mut cut = left.min(block.len());
            while cut > 0 && !block.is_char_boundary(cut) {
                cut -= 1;
            }
            if let Some(nl) = block[..cut].rfind('\n') {
                cut = nl;
            }
            block.truncate(cut);
            left = 0;
        }
        let note = format!("\n[truncated by formslot: the result is held under {CAP_TOKENS} tokens (estimate ceil(bytes / 3.6)); narrow the call or use a cursor]");
        if let Some(last) = blocks.iter_mut().rev().find(|b| !b.is_empty()) {
            last.push_str(&note);
        } else {
            blocks.push(note.trim_start().to_string());
        }
    }
    let content: Vec<Value> = blocks.into_iter().filter(|b| !b.is_empty()).map(|b| json!({ "type": "text", "text": b })).collect();
    let mut result = json!({ "content": content, "isError": false });
    if let Some(value) = structured {
        result["structuredContent"] = value;
    }
    result
}

pub fn error_result(message: &str) -> Value {
    json!({ "content": [{ "type": "text", "text": redact::text(message).0 }], "isError": true })
}

pub fn listing(plan: Listing, total: usize, render: &mut dyn FnMut(usize) -> Item) -> Result<Value, String> {
    let start = match plan.cursor {
        Some(c) => decode(c, plan.kind, &plan.print)?,
        None => 0,
    };
    if start > total {
        return Err("The cursor is past the end of the list. Call again without a cursor.".to_string());
    }
    let budget_bytes = (PAGE_TOKENS * 36) / 10;
    let mut used = plan.header.len() + 512;
    let mut texts: Vec<String> = Vec::new();
    let mut data: Vec<Value> = Vec::new();
    let mut index = start;
    while index < total && texts.len() < plan.size.max(1) {
        let item = render(index);
        let cost = item.text.len() + item.data.to_string().len();
        if !texts.is_empty() && used + cost > budget_bytes {
            break;
        }
        used += cost;
        texts.push(item.text);
        data.push(item.data);
        index += 1;
    }
    let next = if index < total { Some(encode(plan.kind, &plan.print, index)) } else { None };
    let mut text = plan.header;
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    for t in &texts {
        text.push_str(t);
        if !t.ends_with('\n') {
            text.push('\n');
        }
    }
    if texts.is_empty() {
        text.push_str(&format!("No {} found.\n", plan.noun));
    }
    if let Some(cursor) = &next {
        text.push_str(&format!(
            "{} more {}; call again with the same arguments and cursor \"{cursor}\" to continue.\n",
            total - index,
            plan.noun
        ));
    }
    let mut structured = plan.fields;
    structured["total"] = json!(total);
    structured["offset"] = json!(start);
    structured["returned"] = json!(data.len());
    structured["next_cursor"] = json!(next);
    structured[plan.key] = Value::Array(data);
    Ok(text_result(vec![text], Some(structured)))
}

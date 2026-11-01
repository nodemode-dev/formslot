use std::time::Duration;

use serde_json::{Value, json};

const URL: &str = "https://api.anthropic.com/v1/messages/count_tokens";
const VERSION: &str = "2023-06-01";
const DEFAULT_MODEL: &str = "claude-sonnet-5-5";
const SERVICE: &str = "formslot";
const ACCOUNT: &str = "anthropic";

pub fn key() -> crate::Result<String> {
    if let Ok(entry) = keyring::Entry::new(SERVICE, ACCOUNT) {
        if let Ok(found) = entry.get_password() {
            let found = found.trim().to_string();
            if !found.is_empty() {
                return Ok(found);
            }
        }
    }
    match std::env::var("ANTHROPIC_API_KEY") {
        Ok(v) if !v.trim().is_empty() => Ok(v.trim().to_string()),
        _ => Err("--count remote needs the keyring entry formslot/anthropic or ANTHROPIC_API_KEY".to_string()),
    }
}

pub fn remote(key: &str, prompt: &str) -> crate::Result<u64> {
    let model = std::env::var("FORMSLOT_COUNT_MODEL").ok().filter(|m| !m.trim().is_empty()).unwrap_or_else(|| DEFAULT_MODEL.to_string());
    let body = json!({ "model": model, "messages": [{ "role": "user", "content": prompt }] }).to_string();
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(30))).build().into();
    let mut response = agent
        .post(URL)
        .header("x-api-key", key)
        .header("anthropic-version", VERSION)
        .header("content-type", "application/json")
        .send(body.as_str())
        .map_err(|e| format!("count_tokens request failed: {e}"))?;
    let text = response.body_mut().read_to_string().map_err(|e| format!("count_tokens response unreadable: {e}"))?;
    let value: Value = serde_json::from_str(&text).map_err(|e| format!("count_tokens response is not JSON: {e}"))?;
    value
        .get("input_tokens")
        .and_then(Value::as_u64)
        .ok_or_else(|| "count_tokens response has no input_tokens".to_string())
}

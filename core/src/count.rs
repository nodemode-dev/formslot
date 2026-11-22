use std::time::Duration;

use serde_json::{Value, json};

const URL: &str = "https://api.anthropic.com/v1/messages/count_tokens";
const VERSION: &str = "2023-06-01";
const DEFAULT_MODEL: &str = "claude-sonnet-5-5";

pub fn key() -> crate::Result<String> {
    crate::keys::get(crate::keys::ANTHROPIC, crate::keys::ANTHROPIC_ENV).map_err(|e| format!("--count remote needs an Anthropic key; {e}"))
}

pub fn remote(key: &str, prompt: &str) -> crate::Result<u64> {
    let model = std::env::var("FORMSLOT_COUNT_MODEL").ok().filter(|m| !m.trim().is_empty()).unwrap_or_else(|| DEFAULT_MODEL.to_string());
    let body = json!({ "model": model, "messages": [{ "role": "user", "content": prompt }] }).to_string();
    let tls = ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build();
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .max_redirects(0)
        .tls_config(tls)
        .build()
        .into();
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

use base64::Engine as _;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub fn current_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub fn generate_state() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("oauth state entropy should be available");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub fn generate_code_verifier() -> String {
    let mut bytes = [0u8; 64];
    getrandom::fill(&mut bytes).expect("pkce verifier entropy should be available");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub fn generate_code_challenge(code_verifier: &str) -> String {
    let digest = Sha256::digest(code_verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

pub fn percent_encode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect::<String>()
}

pub fn parse_query_value(params: &BTreeMap<String, String>, key: &str) -> Option<String> {
    params
        .get(key)
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub fn extract_code_state_from_callback_url(
    callback_url: &str,
) -> (Option<String>, Option<String>) {
    let trimmed = callback_url.trim();
    if trimmed.is_empty() {
        return (None, None);
    }

    let normalized = trimmed.replace("&amp;", "&");
    if let Ok(parsed) = url::Url::parse(normalized.as_str())
        && let Some(query) = parsed.query()
    {
        return extract_code_state_from_query(query);
    }

    let query = if let Some((_, query)) = normalized.split_once('?') {
        query
    } else {
        normalized.trim_start_matches('?')
    };
    extract_code_state_from_query(query)
}

fn extract_code_state_from_query(query: &str) -> (Option<String>, Option<String>) {
    let mut code = None;
    let mut state = None;
    for (name, value) in url::form_urlencoded::parse(query.as_bytes()) {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        match name.as_ref() {
            "code" | "user_code" => code = Some(trimmed.to_string()),
            "state" => state = Some(trimmed.to_string()),
            _ => {}
        }
    }
    (code, state)
}

pub fn resolve_code_and_state(
    params: &BTreeMap<String, String>,
) -> Result<(String, Option<String>), &'static str> {
    let code = parse_query_value(params, "code").or_else(|| parse_query_value(params, "user_code"));
    let state = parse_query_value(params, "state");

    if let Some(code) = code {
        return Ok((code, state));
    }

    if let Some(callback_url) = parse_query_value(params, "callback_url") {
        let (callback_code, callback_state) = extract_code_state_from_callback_url(&callback_url);
        if let Some(code) = callback_code {
            return Ok((code, state.or(callback_state)));
        }
    }

    Err("missing code")
}

use base64::Engine as _;
use gproxy_channel_api::OAuthError;
use sha2::{Digest, Sha256};

pub(super) fn pkce(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

pub(crate) fn digest(value: &str) -> Vec<u8> {
    Sha256::digest(value.as_bytes()).to_vec()
}

pub(super) fn random_url(length: usize) -> Result<String, OAuthError> {
    let mut bytes = vec![0_u8; length];
    getrandom::fill(&mut bytes).map_err(|_| OAuthError::TemporarilyUnavailable)?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

pub(super) fn stable_id(kind: &str, provider_id: i64, user_id: i64) -> String {
    let digest = Sha256::digest(format!("gproxy-codex-{kind}:{provider_id}:{user_id}"));
    let suffix = digest[..12]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("gproxy-{kind}-{suffix}")
}

pub(super) fn cookie<'a>(headers: &'a http::HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get(http::header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .map(str::trim)
        .find_map(|part| part.strip_prefix(&format!("{name}=")))
        .filter(|value| !value.is_empty())
}

pub(super) fn field<'a>(value: &'a serde_json::Value, name: &str) -> Result<&'a str, OAuthError> {
    value
        .get(name)
        .and_then(serde_json::Value::as_str)
        .ok_or(OAuthError::InvalidGrant)
}

pub(crate) fn now() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .expect("system time is after epoch")
        .as_secs() as i64
}

pub(crate) fn store(error: gproxy_store::StoreError) -> OAuthError {
    OAuthError::Store(error.to_string())
}

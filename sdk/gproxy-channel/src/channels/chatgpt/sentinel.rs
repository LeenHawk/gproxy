//! Two-step sentinel challenge flow for chatgpt.com.
//!
//! Sequence:
//! 1. POST `/sentinel/chat-requirements/prepare` with `{p: <config>}`.
//!    Response contains `{prepare_token, proofofwork: {seed, difficulty}, turnstile, persona}`.
//! 2. Solve PoW locally.
//! 3. POST `/sentinel/chat-requirements/finalize` with `{prepare_token, proofofwork}`.
//!    Response contains `{token, persona}` — that `token` is what the backend
//!    expects as `openai-sentinel-chat-requirements-token` on subsequent
//!    `/f/conversation` calls. The same PoW answer is ALSO used as
//!    `openai-sentinel-proof-token`.
//!
//! Turnstile is deliberately NOT sent. Live testing (2026-04, Team account)
//! confirmed the server accepts finalize without a turnstile field.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use wreq::Client;

use super::pow::solve_pow;
use super::prepare_p::{ConfigOptions, build_prepare_p};
use super::session::{standard_headers, warmup};
use crate::response::UpstreamError;

const CHATGPT_ORIGIN: &str = "https://chatgpt.com";
const PREPARE_URL: &str = "https://chatgpt.com/backend-api/sentinel/chat-requirements/prepare";
const FINALIZE_URL: &str = "https://chatgpt.com/backend-api/sentinel/chat-requirements/finalize";

/// Tokens returned by a successful sentinel round.
#[derive(Debug, Clone)]
pub struct SentinelTokens {
    /// Value for `openai-sentinel-chat-requirements-token` header.
    pub chat_req_token: String,
    /// Value for `openai-sentinel-proof-token` header (same PoW answer
    /// that was sent to finalize).
    pub proof_token: String,
    /// Unix millis timestamp at which `chat_req_token` expires (decoded
    /// from the JWT `exp` claim). `0` if we could not decode it.
    pub chat_req_token_expires_at_ms: u64,
    /// Upstream persona classification (e.g. `chatgpt-paid`, `chatgpt-free`).
    pub persona: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PrepareResponse {
    prepare_token: String,
    #[serde(default)]
    persona: Option<String>,
    proofofwork: Option<ProofOfWorkInfo>,
    // turnstile field is present but intentionally ignored.
}

#[derive(Debug, Deserialize)]
struct ProofOfWorkInfo {
    #[serde(default)]
    required: bool,
    seed: Option<String>,
    difficulty: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FinalizeResponse {
    token: String,
    #[serde(default)]
    persona: Option<String>,
}

/// Run prepare → PoW → finalize and return the tokens.
///
/// `client` should be a `wreq::Client` built with `cookie_store(true)`; the
/// same client must be used for subsequent `/f/conversation` calls so the
/// `__cf_bm` cookie established during warmup is reused.
pub async fn run_sentinel(
    client: &Client,
    access_token: &str,
) -> Result<SentinelTokens, UpstreamError> {
    warmup(client, access_token).await?;
    let opts = ConfigOptions::browser_default();
    let p = build_prepare_p(&opts);
    let prep = call_prepare(client, access_token, &p).await?;

    let pow_info = prep
        .proofofwork
        .ok_or_else(|| UpstreamError::Channel("sentinel prepare: missing proofofwork".into()))?;
    let (seed, difficulty) = match (&pow_info.seed, &pow_info.difficulty) {
        (Some(s), Some(d)) => (s.clone(), d.clone()),
        _ if !pow_info.required => (String::new(), String::new()),
        _ => {
            return Err(UpstreamError::Channel(
                "sentinel prepare: proofofwork required but seed/difficulty missing".into(),
            ));
        }
    };

    let proof_token = if seed.is_empty() {
        String::new()
    } else {
        solve_pow(&seed, &difficulty, &opts)
    };

    let fin = call_finalize(client, access_token, &prep.prepare_token, &proof_token).await?;

    let expires_at_ms = decode_jwt_exp_ms(&fin.token).unwrap_or(0);
    Ok(SentinelTokens {
        chat_req_token: fin.token,
        proof_token,
        chat_req_token_expires_at_ms: expires_at_ms,
        persona: fin.persona.or(prep.persona),
    })
}

async fn call_prepare(
    client: &Client,
    access_token: &str,
    p: &str,
) -> Result<PrepareResponse, UpstreamError> {
    let body = serde_json::json!({ "p": p });
    send_json(client, PREPARE_URL, access_token, body).await
}

async fn call_finalize(
    client: &Client,
    access_token: &str,
    prepare_token: &str,
    proof: &str,
) -> Result<FinalizeResponse, UpstreamError> {
    let mut body = serde_json::Map::new();
    body.insert(
        "prepare_token".to_string(),
        serde_json::Value::String(prepare_token.to_string()),
    );
    if !proof.is_empty() {
        body.insert(
            "proofofwork".to_string(),
            serde_json::Value::String(proof.to_string()),
        );
    }
    send_json(client, FINALIZE_URL, access_token, body.into()).await
}

async fn send_json<T: serde::de::DeserializeOwned>(
    client: &Client,
    url: &str,
    access_token: &str,
    body: serde_json::Value,
) -> Result<T, UpstreamError> {
    let mut req = client.post(url).json(&body);
    req = req.headers(standard_headers(access_token).into());
    let resp = req
        .send()
        .await
        .map_err(|e| UpstreamError::Channel(format!("sentinel http: {e}")))?;
    let status = resp.status();
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| UpstreamError::Channel(format!("sentinel read body: {e}")))?;
    if !status.is_success() {
        return Err(UpstreamError::Channel(format!(
            "sentinel {url} {status}: {}",
            String::from_utf8_lossy(&bytes)
                .chars()
                .take(400)
                .collect::<String>()
        )));
    }
    serde_json::from_slice(&bytes)
        .map_err(|e| UpstreamError::Channel(format!("sentinel decode: {e}")))
}

/// Decode the `exp` claim from a JWT-shaped token. Returns unix millis.
fn decode_jwt_exp_ms(token: &str) -> Option<u64> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() < 2 {
        return None;
    }
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    let claims = URL_SAFE_NO_PAD.decode(parts[1]).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&claims).ok()?;
    let exp = v.get("exp")?.as_u64()?;
    Some(exp.saturating_mul(1000))
}

/// Return `true` if `expires_at_ms` is missing, in the past, or within
/// `skew_ms` of now.
pub fn is_expired(expires_at_ms: u64, skew_ms: u64) -> bool {
    if expires_at_ms == 0 {
        return true;
    }
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    expires_at_ms <= now_ms.saturating_add(skew_ms)
}

/// Reference to keep the dead-code analyzer quiet for a const that is
/// also valuable as documentation.
#[allow(dead_code)]
const _ORIGIN_DOC: &str = CHATGPT_ORIGIN;

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

    #[test]
    fn decodes_jwt_exp() {
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"HS256","typ":"JWT"}"#);
        let body = URL_SAFE_NO_PAD.encode(br#"{"exp":1700000000}"#);
        let token = format!("{header}.{body}.sig");
        assert_eq!(decode_jwt_exp_ms(&token), Some(1_700_000_000_000));
    }

    #[test]
    fn is_expired_handles_missing_and_past() {
        assert!(is_expired(0, 60_000));
        assert!(is_expired(1, 60_000));
        let future = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
            + 10 * 60_000;
        assert!(!is_expired(future, 60_000));
    }
}

use crate::response::UpstreamError;
use serde::Deserialize;

#[derive(Debug)]
pub struct OAuth2TokenResponse {
    pub access_token: String,
    pub expires_at_ms: u64,
    pub refresh_token: Option<String>,
}

#[derive(Deserialize)]
struct TokenEndpointResponse {
    access_token: Option<String>,
    expires_in: Option<u64>,
    refresh_token: Option<String>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Exchange a refresh token for a new access token via OAuth2 `refresh_token` grant.
pub async fn refresh_oauth2_token(
    client: &wreq::Client,
    token_url: &str,
    client_id: &str,
    client_secret: &str,
    refresh_token: &str,
) -> Result<OAuth2TokenResponse, UpstreamError> {
    let mut body = format!(
        "grant_type=refresh_token&refresh_token={}",
        urlencoding(refresh_token),
    );
    if !client_id.is_empty() {
        body.push_str(&format!("&client_id={}", urlencoding(client_id)));
    }
    if !client_secret.is_empty() {
        body.push_str(&format!("&client_secret={}", urlencoding(client_secret)));
    }

    let resp = client
        .post(token_url)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .map_err(|e| UpstreamError::Http(format!("oauth2 refresh: {e}")))?;

    let status = resp.status().as_u16();
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| UpstreamError::Http(format!("oauth2 refresh body: {e}")))?;

    if !(200..300).contains(&status) {
        let text = String::from_utf8_lossy(&bytes);
        return Err(UpstreamError::Channel(format!(
            "oauth2 token endpoint status {status}: {text}"
        )));
    }

    let parsed: TokenEndpointResponse = serde_json::from_slice(&bytes)
        .map_err(|e| UpstreamError::Channel(format!("oauth2 token parse: {e}")))?;

    let access_token = parsed
        .access_token
        .filter(|t| !t.is_empty())
        .ok_or_else(|| UpstreamError::Channel("oauth2 response missing access_token".into()))?;

    let expires_in = parsed.expires_in.unwrap_or(3600);
    let expires_at_ms = now_ms().saturating_add(expires_in.saturating_mul(1000));

    Ok(OAuth2TokenResponse {
        access_token,
        expires_at_ms,
        refresh_token: parsed.refresh_token,
    })
}

fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push_str(&format!("%{:02X}", b));
            }
        }
    }
    out
}

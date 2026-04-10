use crate::engine::UpstreamRequestMeta;
use crate::response::UpstreamError;
use serde::Deserialize;
use sha2::{Digest, Sha256};

// OAuth client that matches what the real Claude Code CLI uses. Anthropic
// validates the client_id + scope + redirect_uri combination at the
// `/v1/oauth/<org_uuid>/authorize` step, so these three have to match the
// values the main `claudecode` channel's OAuth flow uses — any drift here
// results in `Invalid client id provided` or `Invalid scope` and the
// cookie exchange fails.
const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const OAUTH_SCOPE: &str =
    "user:profile user:inference user:sessions:claude_code user:mcp_servers user:file_upload";
const DEFAULT_REDIRECT_URI: &str = "https://platform.claude.com/oauth/code/callback";
const OAUTH_BETA: &str = "oauth-2025-04-20";
const API_VERSION: &str = "2023-06-01";
const USER_AGENT: &str = "claude-cli/2.1.89 (cli, external)";

#[derive(Debug, Deserialize)]
pub(crate) struct CookieTokenResponse {
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_in: Option<u64>,
    #[serde(default)]
    pub organization: Option<CookieTokenOrganization>,
    pub error: Option<String>,
    /// Populated by `exchange_tokens_with_cookie` from the bootstrap
    /// org-discovery step (not part of the token endpoint response).
    #[serde(skip)]
    pub account_uuid: Option<String>,
    #[serde(skip)]
    pub user_email: Option<String>,
}

impl CookieTokenResponse {
    pub fn rate_limit_tier(&self) -> Option<String> {
        self.organization
            .as_ref()
            .and_then(|o| o.rate_limit_tier.clone())
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct CookieTokenOrganization {
    #[serde(default)]
    pub rate_limit_tier: Option<String>,
}

/// Exchange a Claude session cookie for OAuth tokens.
///
/// Flow: cookie → org discovery → authorization code → token exchange.
/// Requires a spoof client (browser TLS fingerprint) to be accepted.
/// Tracked HTTP metadata is pushed to `tracked` for upstream logging.
pub(crate) async fn exchange_tokens_with_cookie(
    client: &wreq::Client,
    api_base_url: &str,
    claude_ai_base_url: &str,
    cookie: &str,
    tracked: &mut Vec<UpstreamRequestMeta>,
) -> Result<CookieTokenResponse, UpstreamError> {
    let api_base = api_base_url.trim_end_matches('/');
    let ai_base = claude_ai_base_url.trim_end_matches('/');

    // Step 1: Get organization info (UUID + billing/rate-limit metadata)
    let org = fetch_org_info(client, cookie, ai_base, tracked).await?;

    // Step 2: Get authorization code with PKCE
    let code_verifier = generate_code_verifier();
    let code_challenge = generate_code_challenge(&code_verifier);
    let state = crate::utils::oauth::generate_state();

    let auth_url = format!("{api_base}/v1/oauth/{}/authorize", org.uuid);
    let payload = serde_json::json!({
        "response_type": "code",
        "client_id": CLIENT_ID,
        "organization_uuid": org.uuid,
        "redirect_uri": DEFAULT_REDIRECT_URI,
        "scope": OAUTH_SCOPE,
        "state": state,
        "code_challenge": code_challenge,
        "code_challenge_method": "S256",
    });

    let auth_req_body = serde_json::to_vec(&payload).unwrap_or_default();
    let auth_start = std::time::Instant::now();
    let response = client
        .post(&auth_url)
        .headers(build_cookie_headers(cookie, ai_base)?)
        .header("content-type", "application/json")
        .body(auth_req_body.clone())
        .send()
        .await
        .map_err(|e| UpstreamError::Http(e.to_string()))?;

    let status = response.status().as_u16();
    let resp_headers = response.headers().clone();
    let body = response
        .bytes()
        .await
        .map_err(|e| UpstreamError::Http(e.to_string()))?;
    track_exchange(
        tracked,
        "POST",
        &auth_url,
        Some(auth_req_body),
        status,
        &resp_headers,
        &body,
        auth_start,
    );
    if !(200..300).contains(&status) {
        return Err(UpstreamError::Channel(format!(
            "cookie auth: authorize endpoint status {status}: {}",
            String::from_utf8_lossy(&body)
        )));
    }
    let auth_response: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|e| UpstreamError::Channel(format!("cookie auth response parse error: {e}")))?;

    let redirect_uri = auth_response
        .get("redirect_uri")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            UpstreamError::Channel(format!(
                "cookie auth: missing redirect_uri in authorize response: {}",
                String::from_utf8_lossy(&body)
            ))
        })?;
    let code = extract_query_param(redirect_uri, "code").ok_or_else(|| {
        UpstreamError::Channel("cookie auth: missing code in redirect_uri".into())
    })?;

    // Step 3: Exchange code for tokens.
    //
    // The token endpoint requires the same header set as the main
    // claudecode OAuth flow — `anthropic-version`, `anthropic-beta:
    // oauth-2025-04-20`, `accept`, `origin`, and a claude-cli `user-agent`.
    // The `state` parameter also has to be in the body (not just the
    // authorize step). Without these, Anthropic returns
    // `invalid_request_error: Invalid request format`.
    let token_url = format!("{api_base}/v1/oauth/token");
    let token_body = format!(
        "grant_type=authorization_code&client_id={}&code={}&redirect_uri={}&code_verifier={}&state={}",
        urlencoding(CLIENT_ID),
        urlencoding(&code),
        urlencoding(DEFAULT_REDIRECT_URI),
        urlencoding(&code_verifier),
        urlencoding(&state),
    );

    let token_start = std::time::Instant::now();
    let token_response = client
        .post(&token_url)
        .header("anthropic-version", API_VERSION)
        .header("anthropic-beta", OAUTH_BETA)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "application/json, text/plain, */*")
        .header("origin", ai_base)
        .header("user-agent", USER_AGENT)
        .body(token_body.clone())
        .send()
        .await
        .map_err(|e| UpstreamError::Http(e.to_string()))?;

    let token_status = token_response.status().as_u16();
    let token_resp_headers = token_response.headers().clone();
    let token_bytes = token_response
        .bytes()
        .await
        .map_err(|e| UpstreamError::Http(e.to_string()))?;
    track_exchange(
        tracked,
        "POST",
        &token_url,
        Some(token_body.into_bytes()),
        token_status,
        &token_resp_headers,
        &token_bytes,
        token_start,
    );
    if !(200..300).contains(&token_status) {
        return Err(UpstreamError::Channel(format!(
            "cookie token endpoint status {token_status}: {}",
            String::from_utf8_lossy(&token_bytes)
        )));
    }
    let mut tokens: CookieTokenResponse = serde_json::from_slice(&token_bytes)
        .map_err(|e| UpstreamError::Channel(format!("cookie token response parse error: {e}")))?;

    // Backfill individual organization metadata fields from bootstrap
    // when the token endpoint didn't return them. The token endpoint may
    // return `"organization": {}` which deserializes as Some with all
    // inner fields None, so we fill per-field rather than checking
    // is_none() on the outer Option.
    let org_data = tokens
        .organization
        .get_or_insert(CookieTokenOrganization {
            rate_limit_tier: None,
        });
    if org_data.rate_limit_tier.is_none() {
        org_data.rate_limit_tier = org.rate_limit_tier;
    }

    if let Some(error) = &tokens.error {
        return Err(UpstreamError::Channel(format!(
            "cookie token exchange error: {error}"
        )));
    }

    tokens.account_uuid = Some(org.uuid);
    tokens.user_email = org.user_email;

    Ok(tokens)
}

struct OrgInfo {
    uuid: String,
    rate_limit_tier: Option<String>,
    user_email: Option<String>,
}

async fn fetch_org_info(
    client: &wreq::Client,
    cookie: &str,
    claude_ai_base_url: &str,
    tracked: &mut Vec<UpstreamRequestMeta>,
) -> Result<OrgInfo, UpstreamError> {
    let bootstrap_url = format!("{claude_ai_base_url}/api/bootstrap");
    let bootstrap_start = std::time::Instant::now();
    let response = client
        .get(&bootstrap_url)
        .headers(build_cookie_headers(cookie, claude_ai_base_url)?)
        .send()
        .await
        .map_err(|e| UpstreamError::Http(e.to_string()))?;

    let status = response.status().as_u16();
    let resp_headers = response.headers().clone();
    let body = response
        .bytes()
        .await
        .map_err(|e| UpstreamError::Http(e.to_string()))?;
    track_exchange(
        tracked,
        "GET",
        &bootstrap_url,
        None,
        status,
        &resp_headers,
        &body,
        bootstrap_start,
    );
    if !(200..300).contains(&status) {
        return Err(UpstreamError::Channel(format!(
            "cookie auth: /api/bootstrap status {status}: {}",
            String::from_utf8_lossy(&body[..body.len().min(400)])
        )));
    }
    let value: serde_json::Value = serde_json::from_slice(&body).map_err(|e| {
        UpstreamError::Channel(format!(
            "bootstrap parse error: {e}: body preview: {}",
            String::from_utf8_lossy(&body[..body.len().min(400)])
        ))
    })?;

    // Try bootstrap response first
    let user_email = value
        .get("account")
        .and_then(|a| a.get("email_address"))
        .and_then(|v| v.as_str())
        .map(String::from);

    if let Some(org_obj) = value
        .get("account")
        .and_then(|a| a.get("memberships"))
        .and_then(|m| m.as_array())
        .and_then(|arr| arr.iter().find_map(|m| m.get("organization")))
        && let Some(uuid) = org_obj.get("uuid").and_then(|u| u.as_str()) {
            return Ok(OrgInfo {
                uuid: uuid.to_string(),
                rate_limit_tier: org_obj
                    .get("rate_limit_tier")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                user_email,
            });
        }

    // Fallback: try /api/organizations
    let orgs_url = format!("{claude_ai_base_url}/api/organizations");
    let orgs_start = std::time::Instant::now();
    let response = client
        .get(&orgs_url)
        .headers(build_cookie_headers(cookie, claude_ai_base_url)?)
        .send()
        .await
        .map_err(|e| UpstreamError::Http(e.to_string()))?;

    let orgs_status = response.status().as_u16();
    let orgs_resp_headers = response.headers().clone();
    let body = response
        .bytes()
        .await
        .map_err(|e| UpstreamError::Http(e.to_string()))?;
    track_exchange(
        tracked,
        "GET",
        &orgs_url,
        None,
        orgs_status,
        &orgs_resp_headers,
        &body,
        orgs_start,
    );
    let orgs: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|e| UpstreamError::Channel(format!("organizations parse error: {e}")))?;

    orgs.as_array()
        .and_then(|arr| {
            arr.iter().find_map(|o| {
                let caps = o.get("capabilities")?.as_array()?;
                if caps.iter().any(|c| c.as_str() == Some("chat")) {
                    let uuid = o.get("uuid").and_then(|u| u.as_str())?.to_string();
                    Some(OrgInfo {
                        uuid,
                        rate_limit_tier: o
                            .get("rate_limit_tier")
                            .and_then(|v| v.as_str())
                            .map(String::from),
                        user_email: user_email.clone(),
                    })
                } else {
                    None
                }
            })
        })
        .ok_or_else(|| UpstreamError::Channel("cookie auth: no chat-capable organization".into()))
}

fn build_cookie_headers(
    cookie: &str,
    claude_ai_base_url: &str,
) -> Result<http::HeaderMap, UpstreamError> {
    let mut headers = http::HeaderMap::new();
    headers.insert("accept", http::HeaderValue::from_static("application/json"));
    headers.insert(
        "accept-language",
        http::HeaderValue::from_static("en-US,en;q=0.9"),
    );
    headers.insert("cache-control", http::HeaderValue::from_static("no-cache"));
    headers.insert(
        "cookie",
        http::HeaderValue::from_str(&format!("sessionKey={cookie}"))
            .map_err(|e| UpstreamError::RequestBuild(e.to_string()))?,
    );
    let origin = claude_ai_base_url.trim_end_matches('/');
    headers.insert(
        "origin",
        http::HeaderValue::from_str(origin)
            .map_err(|e| UpstreamError::RequestBuild(e.to_string()))?,
    );
    headers.insert(
        "referer",
        http::HeaderValue::from_str(&format!("{origin}/new"))
            .map_err(|e| UpstreamError::RequestBuild(e.to_string()))?,
    );
    Ok(headers)
}

fn generate_code_verifier() -> String {
    use rand::RngExt;
    let bytes: Vec<u8> = (0..32).map(|_| rand::rng().random::<u8>()).collect();
    base64_url_encode(&bytes)
}

fn generate_code_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    base64_url_encode(&digest)
}

fn base64_url_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data)
}

fn urlencoding(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

fn extract_query_param(url: &str, key: &str) -> Option<String> {
    let query = url.split_once('?')?.1;
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == key).then(|| v.to_string())
    })
}

fn track_exchange(
    tracked: &mut Vec<UpstreamRequestMeta>,
    method: &str,
    url: &str,
    request_body: Option<Vec<u8>>,
    status: u16,
    response_headers: &http::HeaderMap,
    response_body: &[u8],
    start: std::time::Instant,
) {
    tracked.push(UpstreamRequestMeta {
        method: method.to_string(),
        url: url.to_string(),
        request_headers: Vec::new(),
        request_body,
        response_status: Some(status),
        response_headers: response_headers
            .iter()
            .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
            .collect(),
        response_body: Some(response_body.to_vec()),
        model: None,
        latency_ms: start.elapsed().as_millis() as u64,
        credential_index: None,
    });
}

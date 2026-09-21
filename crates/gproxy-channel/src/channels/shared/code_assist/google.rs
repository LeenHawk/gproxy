//! Google account login, refresh and the one-time Code Assist onboarding the
//! two Gemini CLI-impersonating channels share.
//!
//! Wire facts follow v3 `crates/gproxy-channels/src/shared/{google_login,
//! google_oauth}.rs` on `main`: a standard Google installed-app authorization
//! code flow with PKCE (`accounts.google.com/o/oauth2/v2/auth`,
//! `oauth2.googleapis.com/token`, `access_type=offline`, `prompt=consent`),
//! form-encoded token bodies carrying the tool's own client id *and* secret,
//! and, once an access token exists, `POST {base}/v1internal:loadCodeAssist`
//! followed if needed by `POST {base}/v1internal:onboardUser` to learn the
//! Cloud project and the subscription tier.
//!
//! These facts are learned once, at login, and travel in
//! `OAuthCredential::provider_fields`; the host writes them onto the
//! credential row and `prepare` reads them back from the metadata. Nothing
//! here is called per request.

use super::{encode, invalid_request, invalid_response, send, text, unix_now_ms};
use crate::OutboundClient;
use crate::channel::{
    AuthorizationRequest, AuthorizationStart, ChannelError, CredentialUpdate, OAuthCredential,
};
use http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// One tool's Google identity and Code Assist host. Borrowed for the
/// duration of one login or refresh; a provider may override any of it.
pub(crate) struct GoogleTool<'a> {
    /// The tool's own OAuth client id.
    pub client_id: &'a str,
    /// Installed-app "secret"; Google ships it inside the tool, so it is an
    /// identifier rather than a secret, and the token endpoint requires it.
    pub client_secret: &'a str,
    pub token_url: &'a str,
    /// Where Google sends the code back when the host names no redirect.
    pub redirect_uri: &'a str,
    pub scope: &'a str,
    /// Tier id used for onboarding when `loadCodeAssist` names no default.
    pub fallback_tier: &'a str,
    /// The tool's user agent, which the Code Assist host uses to tell the
    /// tools apart.
    pub user_agent: &'a str,
    /// `ClientMetadata` for `loadCodeAssist`/`onboardUser`; the two tools
    /// identify themselves differently.
    pub metadata: fn(Option<&str>) -> Value,
}

fn header_value(value: &str) -> Result<HeaderValue, ChannelError> {
    HeaderValue::from_str(value).map_err(|_| ChannelError::InvalidCredential)
}

/// Percent-encode a query or form component (v3 `shared/http.rs::form`).
pub(crate) fn encode_component(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut output = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            output.push(char::from(byte));
        } else {
            output.push('%');
            output.push(char::from(HEX[usize::from(byte >> 4)]));
            output.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
    output
}

pub(crate) fn form_encode(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(name, value)| format!("{}={}", encode_component(name), encode_component(value)))
        .collect::<Vec<_>>()
        .join("&")
}

fn form_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/x-www-form-urlencoded"),
    );
    headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
    headers
}

/// The browser step. The host owns the PKCE verifier and `state`; only the
/// S256 challenge reaches Google.
pub(crate) fn authorize(
    tool: &GoogleTool<'_>,
    authorize_url: &str,
    request: AuthorizationRequest<'_>,
) -> AuthorizationStart {
    let redirect_uri = match request.redirect_uri.trim() {
        "" => tool.redirect_uri,
        given => given,
    };
    let query = form_encode(&[
        ("response_type", "code"),
        ("client_id", tool.client_id),
        ("redirect_uri", redirect_uri),
        ("scope", tool.scope),
        ("access_type", "offline"),
        ("prompt", "consent"),
        ("code_challenge_method", "S256"),
        ("code_challenge", request.code_challenge),
        ("state", request.state),
    ]);
    AuthorizationStart {
        authorize_url: format!("{}?{query}", authorize_url.trim_end_matches('?')),
        redirect_uri: redirect_uri.to_owned(),
    }
}

/// Google's token endpoint answers `{"error": "invalid_grant", ...}`; that,
/// and a 401, mean the grant is gone for good rather than temporarily
/// unavailable.
fn token_error(status: StatusCode, body: &[u8]) -> Option<String> {
    let code = serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| text(&value, "error").map(str::to_ascii_lowercase));
    let definitive = status == StatusCode::UNAUTHORIZED
        || (status.is_client_error()
            && matches!(
                code.as_deref(),
                Some("invalid_grant" | "invalid_client" | "unauthorized_client")
            ));
    definitive.then(|| code.unwrap_or_else(|| format!("http {}", status.as_u16())))
}

struct Tokens {
    access_token: String,
    refresh_token: Option<String>,
    id_token: Option<String>,
    scopes: Vec<String>,
    expires_at_ms: Option<i64>,
}

async fn token_request(
    client: &dyn OutboundClient,
    token_url: &str,
    pairs: &[(&str, &str)],
    // A definitive refusal means "this credential is finished" only when an
    // existing credential is being renewed; during a login it is just a
    // failed attempt the operator repeats.
    renewing: bool,
) -> Result<Tokens, ChannelError> {
    let (status, _, bytes) = send(
        client,
        Method::POST,
        token_url,
        form_headers(),
        Some(form_encode(pairs).into_bytes()),
    )
    .await?;
    if !status.is_success() {
        if renewing && let Some(code) = token_error(status, &bytes) {
            return Err(ChannelError::RefreshRejected(code));
        }
        return Err(ChannelError::UpstreamResponse {
            status,
            body: bytes,
        });
    }
    let token: Value = serde_json::from_slice(&bytes)
        .map_err(|error| invalid_response(format!("Google token response JSON: {error}")))?;
    let access_token = text(&token, "access_token")
        .ok_or_else(|| invalid_response("Google token response has no access_token"))?
        .to_owned();
    Ok(Tokens {
        access_token,
        refresh_token: text(&token, "refresh_token").map(str::to_owned),
        id_token: text(&token, "id_token").map(str::to_owned),
        scopes: text(&token, "scope")
            .map(|scope| scope.split_whitespace().map(str::to_owned).collect())
            .unwrap_or_default(),
        expires_at_ms: token
            .get("expires_in")
            .and_then(Value::as_i64)
            .or(Some(3_600))
            .map(|seconds| unix_now_ms().saturating_add(seconds.max(0).saturating_mul(1_000))),
    })
}

/// The `authorization_code` grant plus the account facts a login discovers.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn exchange(
    client: &dyn OutboundClient,
    tool: &GoogleTool<'_>,
    code_assist_base: &str,
    code: &str,
    redirect_uri: &str,
    code_verifier: &str,
    project_hint: Option<&str>,
) -> Result<OAuthCredential, ChannelError> {
    let redirect_uri = match redirect_uri.trim() {
        "" => tool.redirect_uri,
        given => given,
    };
    let tokens = token_request(
        client,
        tool.token_url,
        &[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", tool.client_id),
            ("client_secret", tool.client_secret),
            ("code_verifier", code_verifier),
        ],
        false,
    )
    .await?;
    if tokens.refresh_token.is_none() {
        return Err(invalid_response(
            "Google token response has no refresh_token; the consent screen must be re-approved",
        ));
    }
    let mut fields = BTreeMap::new();
    discover(
        client,
        tool,
        code_assist_base,
        &tokens.access_token,
        project_hint,
        &mut fields,
    )
    .await?;
    enrich_email(client, tool, &tokens.access_token, &mut fields).await;
    Ok(credential(tokens, None, fields))
}

/// The `refresh_token` grant. Google rotates neither the refresh token nor
/// the scope list on a normal refresh, so both fall back to what the
/// credential already had, and the account facts carry over untouched unless
/// the login never managed to learn them (see `discover`).
pub(crate) async fn refresh(
    client: &dyn OutboundClient,
    tool: &GoogleTool<'_>,
    code_assist_base: &str,
    secret: &Value,
    project_hint: Option<&str>,
) -> Result<CredentialUpdate, ChannelError> {
    let refresh_token = text(secret, "refresh_token").ok_or_else(|| {
        ChannelError::RefreshRejected("credential has no refresh token".to_owned())
    })?;
    let tokens = token_request(
        client,
        tool.token_url,
        &[
            ("grant_type", "refresh_token"),
            ("client_id", tool.client_id),
            ("client_secret", tool.client_secret),
            ("refresh_token", refresh_token),
        ],
        true,
    )
    .await?;
    let mut fields = previous_fields(secret);
    // A login that ended before the Code Assist onboarding operation
    // finished leaves no project behind; the first refresh completes it, so
    // discovery stays out of the synchronous, per-request `prepare`.
    if !fields.contains_key("project_id") {
        discover(
            client,
            tool,
            code_assist_base,
            &tokens.access_token,
            project_hint,
            &mut fields,
        )
        .await?;
    }
    let credential = credential(tokens, Some(refresh_token.to_owned()), fields);
    Ok(CredentialUpdate {
        expires_at_ms: credential.expires_at_ms,
        secret: encode_secret(&credential)?,
    })
}

fn credential(
    tokens: Tokens,
    previous_refresh: Option<String>,
    provider_fields: BTreeMap<String, Value>,
) -> OAuthCredential {
    OAuthCredential {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token.or(previous_refresh),
        id_token: tokens.id_token,
        token_type: Some("Bearer".into()),
        scopes: tokens.scopes,
        expires_at_ms: tokens.expires_at_ms,
        refresh_expires_at_ms: None,
        provider_fields,
    }
}

pub(crate) fn encode_secret(credential: &OAuthCredential) -> Result<Value, ChannelError> {
    serde_json::to_value(credential).map_err(|error| invalid_response(error.to_string()))
}

/// Account facts already on the secret: the `provider_fields` envelope first,
/// then the flat layout v3 wrote.
pub(crate) fn previous_fields(secret: &Value) -> BTreeMap<String, Value> {
    let mut fields = BTreeMap::new();
    if let Some(Value::Object(previous)) = secret.get("provider_fields") {
        for (key, value) in previous {
            fields.insert(key.clone(), value.clone());
        }
    }
    for name in ["project_id", "rate_limit_tier", "user_email"] {
        if let Some(value) = text(secret, name)
            && !fields.contains_key(name)
        {
            fields.insert(name.into(), Value::String(value.into()));
        }
    }
    fields
}

async fn code_assist_post(
    client: &dyn OutboundClient,
    tool: &GoogleTool<'_>,
    base: &str,
    access_token: &str,
    path: &str,
    body: &Value,
) -> Result<Value, ChannelError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        header_value(&format!("Bearer {access_token}"))?,
    );
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(header::USER_AGENT, header_value(tool.user_agent)?);
    let (status, _, bytes) = send(
        client,
        Method::POST,
        &format!("{}{path}", base.trim_end_matches('/')),
        headers,
        Some(encode(body, invalid_request)?.to_vec()),
    )
    .await?;
    if !status.is_success() {
        return Err(ChannelError::UpstreamResponse {
            status,
            body: bytes,
        });
    }
    serde_json::from_slice(&bytes)
        .map_err(|error| invalid_response(format!("Code Assist response JSON: {error}")))
}

fn project_id(value: &Value) -> Option<String> {
    value
        .as_str()
        .or_else(|| value.get("id").and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn default_tier<'a>(loaded: &'a Value, fallback: &'a str) -> &'a str {
    loaded
        .get("allowedTiers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|tier| tier.get("isDefault").and_then(Value::as_bool) == Some(true))
        .and_then(|tier| text(tier, "id"))
        .unwrap_or(fallback)
}

/// The plan name the account is on, folded onto the three names the rest of
/// gproxy uses (v3 `subscription_tier`).
fn subscription_tier(loaded: &Value) -> Option<String> {
    let raw = ["paidTier", "currentTier"]
        .into_iter()
        .find_map(|name| loaded.get(name).and_then(|tier| text(tier, "id")))?;
    Some(match raw.to_ascii_lowercase().as_str() {
        "g1-ultra-tier" | "ws-ai-ultra-business-tier" => "ultra".into(),
        "free-tier" => "free".into(),
        _ => "pro".into(),
    })
}

/// `loadCodeAssist`, then `onboardUser` when the account has no project yet.
///
/// v3 polled the long-running onboarding operation every five seconds for up
/// to five minutes. A v4 channel has no timer (and `LoginContext` gives it
/// none), so a still-running operation is reported rather than spun on: the
/// operator retries the login, or sets `project_id` on the provider, and in
/// the meantime the next `CredentialRefresh` finishes the discovery, since
/// the operation completes server side.
async fn discover(
    client: &dyn OutboundClient,
    tool: &GoogleTool<'_>,
    base: &str,
    access_token: &str,
    hint: Option<&str>,
    fields: &mut BTreeMap<String, Value>,
) -> Result<(), ChannelError> {
    let hint = hint.map(str::trim).filter(|hint| !hint.is_empty());
    let metadata = (tool.metadata)(hint);
    let mut load = json!({ "metadata": metadata });
    if let Some(project) = hint {
        load["cloudaicompanionProject"] = Value::String(project.into());
    }
    let loaded = code_assist_post(
        client,
        tool,
        base,
        access_token,
        "/v1internal:loadCodeAssist",
        &load,
    )
    .await?;
    if let Some(tier) = subscription_tier(&loaded) {
        fields.insert("rate_limit_tier".into(), Value::String(tier));
    }
    if let Some(project) = loaded.get("cloudaicompanionProject").and_then(project_id) {
        fields.insert("project_id".into(), Value::String(project));
        return Ok(());
    }
    let mut onboard = json!({
        "tierId": default_tier(&loaded, tool.fallback_tier),
        "metadata": metadata,
    });
    if let Some(project) = hint {
        onboard["cloudaicompanionProject"] = Value::String(project.into());
    }
    let onboarded = code_assist_post(
        client,
        tool,
        base,
        access_token,
        "/v1internal:onboardUser",
        &onboard,
    )
    .await?;
    let project = onboarded
        .pointer("/response/cloudaicompanionProject")
        .and_then(project_id)
        .or_else(|| {
            onboarded
                .get("cloudaicompanionProject")
                .and_then(project_id)
        })
        .or_else(|| hint.map(str::to_owned));
    match project {
        Some(project) => {
            fields.insert("project_id".into(), Value::String(project));
            Ok(())
        }
        None if onboarded.get("done").and_then(Value::as_bool) == Some(false) => {
            Err(invalid_response(
                "Code Assist onboarding is still running; retry the login once it completes, \
                 or set project_id on the provider",
            ))
        }
        None => Err(invalid_response(
            "Code Assist returned no project; retry the login or set project_id on the provider",
        )),
    }
}

/// The signed-in address, for a readable credential label. Best effort: a
/// failure leaves the fields untouched.
async fn enrich_email(
    client: &dyn OutboundClient,
    tool: &GoogleTool<'_>,
    access_token: &str,
    fields: &mut BTreeMap<String, Value>,
) {
    let mut headers = HeaderMap::new();
    let Ok(bearer) = header_value(&format!("Bearer {access_token}")) else {
        return;
    };
    headers.insert(header::AUTHORIZATION, bearer);
    headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
    let Ok(agent) = header_value(tool.user_agent) else {
        return;
    };
    headers.insert(header::USER_AGENT, agent);
    let Ok((status, _, bytes)) = send(
        client,
        Method::GET,
        "https://www.googleapis.com/oauth2/v1/userinfo?alt=json",
        headers,
        None,
    )
    .await
    else {
        return;
    };
    if !status.is_success() {
        return;
    }
    if let Ok(profile) = serde_json::from_slice::<Value>(&bytes)
        && let Some(email) = text(&profile, "email")
    {
        fields.insert("user_email".into(), Value::String(email.into()));
    }
}

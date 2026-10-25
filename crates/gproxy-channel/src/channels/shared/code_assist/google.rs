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

use super::{invalid_response, send, text, unix_now_ms};
use crate::OutboundClient;
use crate::channel::{
    AuthorizationRequest, AuthorizationStart, ChannelError, CredentialUpdate, OAuthCredential,
};
use crate::channels::shared::timer::sleep;
use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, header};
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::Duration;

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
    /// The consent URL's parameters, in the order the tool's
    /// google-auth-library `generateAuthUrl` emits them: the keys the tool
    /// passes first, then `response_type`, `client_id` (and `redirect_uri`
    /// when it came from the client's constructor). Known keys are
    /// `redirect_uri`, `access_type`, `scope`, `prompt`,
    /// `code_challenge_method`, `code_challenge`, `state`, `response_type`
    /// and `client_id`. A tool that lists `code_challenge` uses PKCE, and its
    /// code exchange carries the verifier; one that does not sends neither.
    pub authorize_params: &'a [&'a str],
    /// The google-auth-library banner its token calls carry as their whole
    /// `User-Agent` (`google-api-nodejs-client/<version>`).
    pub auth_library: &'a str,
    /// The Node runtime google-auth-library reports in `x-goog-api-client`
    /// (`gl-node/<version>`) on its token calls.
    pub node_client: &'a str,
    /// `x-goog-api-client` on `loadCodeAssist`/`onboardUser`, for a tool that
    /// sends them through google-auth-library's `OAuth2Client.request`.
    pub code_assist_api_client: Option<&'a str>,
    /// `Accept` on `loadCodeAssist`/`onboardUser`: gaxios asks for
    /// `application/json` only when the caller sets `responseType: 'json'`,
    /// and node-fetch's `*/*` stands otherwise.
    pub code_assist_accept: &'static str,
    /// What the tool sends beside `Authorization` on its `userinfo` call.
    pub userinfo_headers: &'a [(&'static str, &'a str)],
    /// What the tool does when `loadCodeAssist` names no project: use the
    /// hint, onboard on some tier, or refuse. The two tools disagree, so the
    /// rule is theirs (`loaded`, then the project hint).
    pub onboarding: fn(&Value, Option<&str>) -> Result<Onboarding, ChannelError>,
    /// How long the tool waits between polls of a running onboarding
    /// operation.
    pub onboard_poll: Duration,
    /// The user agent of the tool's `loadCodeAssist`/`onboardUser` calls,
    /// which the Code Assist host uses to tell the tools apart.
    pub user_agent: &'a str,
    /// `ClientMetadata` for `loadCodeAssist`/`onboardUser`, already encoded
    /// as a JSON object in the tool's own key order (`serde_json` would sort
    /// it); the two tools identify themselves differently.
    pub metadata: fn(Option<&str>) -> String,
}

/// A tool's answer to an account `loadCodeAssist` found without a project.
pub(crate) enum Onboarding {
    /// Use this project as is; no onboarding call.
    Project(String),
    /// `onboardUser` on this tier.
    Tier(String),
}

/// The longest a login waits on a running onboarding operation before it
/// gives up; both tools poll without a limit, a login request cannot.
const ONBOARD_POLL_BUDGET: Duration = Duration::from_secs(60);

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

/// The body google-auth-library posts: a `URLSearchParams`, whose
/// serializer keeps `*-._` and alphanumerics, writes a space as `+` and
/// escapes everything else, `~` included.
fn search_params(pairs: &[(&str, &str)]) -> String {
    fn component(value: &str) -> String {
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        let mut output = String::with_capacity(value.len());
        for byte in value.bytes() {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'*' | b'-' | b'.' | b'_') {
                output.push(char::from(byte));
            } else if byte == b' ' {
                output.push('+');
            } else {
                output.push('%');
                output.push(char::from(HEX[usize::from(byte >> 4)]));
                output.push(char::from(HEX[usize::from(byte & 0x0f)]));
            }
        }
        output
    }
    pairs
        .iter()
        .map(|(name, value)| format!("{}={}", component(name), component(value)))
        .collect::<Vec<_>>()
        .join("&")
}

/// What google-auth-library's token calls carry, in its order: node-fetch's
/// `Accept: */*`, the `URLSearchParams` content type, and the library's own
/// banner and runtime (`authclient.js` `DEFAULT_REQUEST_INTERCEPTOR`).
/// node-fetch also asks for `gzip, deflate, br`; the channel's connection
/// decides what it can decode, so `Accept-Encoding` is left to it.
fn form_headers(tool: &GoogleTool<'_>) -> Result<HeaderMap, ChannelError> {
    let mut headers = HeaderMap::new();
    headers.insert(header::ACCEPT, HeaderValue::from_static("*/*"));
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/x-www-form-urlencoded;charset=UTF-8"),
    );
    headers.insert(header::USER_AGENT, header_value(tool.auth_library)?);
    headers.insert(
        HeaderName::from_static("x-goog-api-client"),
        header_value(tool.node_client)?,
    );
    Ok(headers)
}

/// The browser step. The host owns the PKCE verifier and `state`; only the
/// S256 challenge reaches Google, and only for a tool that uses PKCE.
pub(crate) fn authorize(
    tool: &GoogleTool<'_>,
    authorize_url: &str,
    request: AuthorizationRequest<'_>,
) -> AuthorizationStart {
    let redirect_uri = match request.redirect_uri.trim() {
        "" => tool.redirect_uri,
        given => given,
    };
    let pairs = tool
        .authorize_params
        .iter()
        .filter_map(|&name| {
            let value = match name {
                "redirect_uri" => redirect_uri,
                "access_type" => "offline",
                "scope" => tool.scope,
                "prompt" => "consent",
                "code_challenge_method" => "S256",
                "code_challenge" => request.code_challenge,
                "state" => request.state,
                "response_type" => "code",
                "client_id" => tool.client_id,
                _ => return None,
            };
            Some((name, value))
        })
        .collect::<Vec<_>>();
    let query = form_encode(&pairs);
    AuthorizationStart {
        authorize_url: format!("{}?{query}", authorize_url.trim_end_matches('?')),
        redirect_uri: redirect_uri.to_owned(),
        // Google's client is a constant of the tool being impersonated, so the
        // exchange needs nothing this call learned.
        provider_state: BTreeMap::new(),
    }
}

/// Only `{"error": "invalid_grant", ...}` means this grant is gone for good
/// (revoked, expired, or a reauth demand). The Antigravity IDE logs out on
/// exactly that (`e?.response?.data?.error==="invalid_grant"`), and
/// google-auth-library singles out no other code. `invalid_client`,
/// `unauthorized_client` and a bare 401 concern the tool's shared client, not
/// this account, so they stay retryable rather than killing every credential.
fn token_error(status: StatusCode, body: &[u8]) -> Option<String> {
    let code = serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| text(&value, "error").map(str::to_ascii_lowercase))?;
    (status.is_client_error() && code == "invalid_grant").then_some(code)
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
    tool: &GoogleTool<'_>,
    pairs: &[(&str, &str)],
    // A definitive refusal means "this credential is finished" only when an
    // existing credential is being renewed; during a login it is just a
    // failed attempt the operator repeats.
    renewing: bool,
) -> Result<Tokens, ChannelError> {
    let (status, _, bytes) = send(
        client,
        Method::POST,
        tool.token_url,
        form_headers(tool)?,
        Some(search_params(pairs).into_bytes()),
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
    // google-auth-library `getTokenAsync` field order, the secret appended
    // last (`ClientSecretPost`); `code_verifier` is dropped when undefined,
    // as it is for a tool without PKCE.
    let mut pairs = vec![("client_id", tool.client_id)];
    if tool.authorize_params.contains(&"code_challenge") {
        pairs.push(("code_verifier", code_verifier));
    }
    pairs.extend([
        ("code", code),
        ("grant_type", "authorization_code"),
        ("redirect_uri", redirect_uri),
        ("client_secret", tool.client_secret),
    ]);
    let tokens = token_request(client, tool, &pairs, false).await?;
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
    // google-auth-library `refreshTokenNoCache` field order.
    let tokens = token_request(
        client,
        tool,
        &[
            ("refresh_token", refresh_token),
            ("client_id", tool.client_id),
            ("client_secret", tool.client_secret),
            ("grant_type", "refresh_token"),
        ],
        true,
    )
    .await?;
    let mut fields = previous_fields(secret);
    // A login that ended before the Code Assist onboarding operation
    // finished leaves no project behind; the first refresh completes it, so
    // discovery stays out of the synchronous, per-request `prepare`. A failed
    // discovery must not cost the token just minted: the CLI and the IDE
    // persist refreshed tokens independently of their Code Assist setup, so
    // the update is returned without `project_id` and the next refresh tries
    // again.
    if !fields.contains_key("project_id") {
        let mut discovered = fields.clone();
        if discover(
            client,
            tool,
            code_assist_base,
            &tokens.access_token,
            project_hint,
            &mut discovered,
        )
        .await
        .is_ok()
        {
            fields = discovered;
        }
    }
    let credential = credential(tokens, Some(refresh_token.to_owned()), fields);
    Ok(CredentialUpdate {
        metadata: None,
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
        provider_secrets: BTreeMap::new(),
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

async fn code_assist_call(
    client: &dyn OutboundClient,
    tool: &GoogleTool<'_>,
    base: &str,
    access_token: &str,
    path: &str,
    body: Option<Vec<u8>>,
) -> Result<Value, ChannelError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::ACCEPT,
        HeaderValue::from_static(tool.code_assist_accept),
    );
    headers.insert(
        header::AUTHORIZATION,
        header_value(&format!("Bearer {access_token}"))?,
    );
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(header::USER_AGENT, header_value(tool.user_agent)?);
    if let Some(api_client) = tool.code_assist_api_client {
        headers.insert(
            HeaderName::from_static("x-goog-api-client"),
            header_value(api_client)?,
        );
    }
    let method = if body.is_some() {
        Method::POST
    } else {
        Method::GET
    };
    let (status, _, bytes) = send(
        client,
        method,
        &format!("{}{path}", base.trim_end_matches('/')),
        headers,
        body,
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

/// The id of the tier `loadCodeAssist` marks `isDefault` among `allowedTiers`.
pub(crate) fn default_tier(loaded: &Value) -> Option<&str> {
    loaded
        .get("allowedTiers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|tier| tier.get("isDefault").and_then(Value::as_bool) == Some(true))
        .and_then(|tier| text(tier, "id"))
}

/// Whether `tier_id` is among `allowedTiers`.
pub(crate) fn tier_allowed(loaded: &Value, tier_id: &str) -> bool {
    loaded
        .get("allowedTiers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|tier| text(tier, "id") == Some(tier_id))
}

/// The `reasonMessage` of the `ineligibleTiers` entry for `tier_id`.
pub(crate) fn ineligible_reason<'a>(loaded: &'a Value, tier_id: &str) -> Option<&'a str> {
    loaded
        .get("ineligibleTiers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|tier| text(tier, "tierId") == Some(tier_id))
        .and_then(|tier| text(tier, "reasonMessage"))
}

/// The refusal for an account that has no project and none can be had:
/// the server's ineligibility reasons when it gives any, else a request for
/// a project (gemini-cli `setup.ts::throwIneligibleOrProjectIdError`).
pub(crate) fn project_required(loaded: &Value) -> ChannelError {
    let reasons = loaded
        .get("ineligibleTiers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|tier| text(tier, "reasonMessage"))
        .collect::<Vec<_>>();
    if reasons.is_empty() {
        invalid_response(
            "Code Assist returned no project for this account; set project_id on the provider",
        )
    } else {
        invalid_response(format!(
            "Code Assist: account is not eligible: {}",
            reasons.join(", ")
        ))
    }
}

/// A tier id folded onto the three plan names the rest of gproxy uses (v3
/// `subscription_tier`).
fn fold_tier(raw: &str) -> String {
    match raw.to_ascii_lowercase().as_str() {
        "g1-ultra-tier" | "ws-ai-ultra-business-tier" => "ultra".into(),
        "free-tier" => "free".into(),
        _ => "pro".into(),
    }
}

/// The plan name the account is on.
fn subscription_tier(loaded: &Value) -> Option<String> {
    ["paidTier", "currentTier"]
        .into_iter()
        .find_map(|name| loaded.get(name).and_then(|tier| text(tier, "id")))
        .map(fold_tier)
}

/// `loadCodeAssist`, then, when the account has no project, whatever the
/// tool's `onboarding` rule asks for. An `onboardUser` that answers with a
/// running operation is polled (`GET {base}/v1internal/{name}`) the way both
/// tools do (gemini-cli `setup.ts` every 5 s, the Antigravity IDE every
/// 500 ms), within `ONBOARD_POLL_BUDGET`; an operation still running after
/// that fails the login, which the operator retries.
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
    // `{cloudaicompanionProject?, metadata}`: gemini-cli's object literal and
    // the IDE's `LoadCodeAssistRequest` field order agree.
    let mut load = String::from("{");
    if let Some(project) = hint {
        load.push_str(&format!(
            "\"cloudaicompanionProject\":{},",
            Value::String(project.into())
        ));
    }
    load.push_str(&format!("\"metadata\":{metadata}}}"));
    let loaded = code_assist_call(
        client,
        tool,
        base,
        access_token,
        "/v1internal:loadCodeAssist",
        Some(load.into_bytes()),
    )
    .await?;
    if let Some(tier) = subscription_tier(&loaded) {
        fields.insert("rate_limit_tier".into(), Value::String(tier));
    }
    if let Some(project) = loaded.get("cloudaicompanionProject").and_then(project_id) {
        fields.insert("project_id".into(), Value::String(project));
        return Ok(());
    }
    let tier = match (tool.onboarding)(&loaded, hint)? {
        Onboarding::Project(project) => {
            fields.insert("project_id".into(), Value::String(project));
            return Ok(());
        }
        Onboarding::Tier(tier) => tier,
    };
    // `{tierId, cloudaicompanionProject?, metadata}` in the order both tools
    // build it (gemini-cli `setup.ts` `onboardReq`); a `json!` map would sort
    // the keys.
    let mut onboard = format!("{{\"tierId\":{}", Value::String(tier.clone()));
    if let Some(project) = hint {
        onboard.push_str(&format!(
            ",\"cloudaicompanionProject\":{}",
            Value::String(project.into())
        ));
    }
    onboard.push_str(&format!(",\"metadata\":{metadata}}}"));
    let mut operation = code_assist_call(
        client,
        tool,
        base,
        access_token,
        "/v1internal:onboardUser",
        Some(onboard.into_bytes()),
    )
    .await?;
    let polls = (ONBOARD_POLL_BUDGET.as_millis() / tool.onboard_poll.as_millis().max(1)) as usize;
    for _ in 0..polls {
        if operation.get("done").and_then(Value::as_bool) != Some(false) {
            break;
        }
        let Some(name) = text(&operation, "name").map(str::to_owned) else {
            break;
        };
        sleep(tool.onboard_poll).await;
        operation = code_assist_call(
            client,
            tool,
            base,
            access_token,
            &format!("/v1internal/{}", name.trim_start_matches('/')),
            None,
        )
        .await?;
    }
    if operation.get("done").and_then(Value::as_bool) == Some(false) {
        return Err(invalid_response(
            "Code Assist onboarding is still running; retry the login once it completes, \
             or set project_id on the provider",
        ));
    }
    // The tier the account was just onboarded on is its tier from now on
    // (gemini-cli `setup.ts`: `userTier: tier.id`).
    fields
        .entry("rate_limit_tier".into())
        .or_insert_with(|| Value::String(fold_tier(&tier)));
    let project = operation
        .pointer("/response/cloudaicompanionProject")
        .and_then(project_id)
        .or_else(|| {
            operation
                .get("cloudaicompanionProject")
                .and_then(project_id)
        })
        .or_else(|| hint.map(str::to_owned));
    match project {
        Some(project) => {
            fields.insert("project_id".into(), Value::String(project));
            Ok(())
        }
        None => Err(project_required(&loaded)),
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
    for (name, value) in tool.userinfo_headers {
        let Ok(value) = header_value(value) else {
            return;
        };
        headers.insert(HeaderName::from_static(name), value);
    }
    // Both tools read the v2 endpoint (gemini-cli `fetchAndCacheUserInfo`,
    // the Antigravity IDE's `_profileDataUrl`).
    let Ok((status, _, bytes)) = send(
        client,
        Method::GET,
        "https://www.googleapis.com/oauth2/v2/userinfo",
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

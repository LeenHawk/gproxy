use bytes::Bytes;
use gproxy_channel_api::{
    BoxFuture, ChannelError, OAuthAuthorizeInput, OAuthError, SurfaceBody, SurfaceReply,
    SurfaceServices, SynthCtx, Synthesizer,
};
use http::header::{CACHE_CONTROL, CONTENT_TYPE, LOCATION};
use http::{HeaderMap, HeaderValue, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use super::super::auth::CLIENT_ID;

const SCOPES: &[&str] = &[
    "openid",
    "profile",
    "email",
    "offline_access",
    "api.connectors.read",
    "api.connectors.invoke",
];

pub(super) static HANDLER: OAuth = OAuth;

pub(super) struct OAuth;

impl Synthesizer for OAuth {
    fn respond<'a>(
        &'a self,
        ctx: SynthCtx<'a>,
        services: SurfaceServices<'a>,
    ) -> BoxFuture<'a, Result<SurfaceReply, ChannelError>> {
        Box::pin(async move {
            let oauth = services
                .oauth
                .ok_or_else(|| ChannelError::Prepare("OAuth issuer unavailable".into()))?;
            match (ctx.method.as_str(), ctx.path) {
                ("GET", "/oauth/authorize") => authorize_get(ctx, oauth).await,
                ("POST", "/oauth/authorize") => {
                    authorize_post(ctx, services.provider.id, oauth).await
                }
                ("POST", "/oauth/token") => token(ctx, oauth).await,
                ("POST", "/oauth/revoke") => revoke(ctx, oauth).await,
                ("POST", "/api/accounts/deviceauth/usercode") => {
                    device_start(ctx, services.provider.id, oauth).await
                }
                ("POST", "/api/accounts/deviceauth/token") => device_poll(ctx, oauth).await,
                ("GET", "/codex/device") | ("POST", "/codex/device") => {
                    device_page(ctx, oauth).await
                }
                _ => Err(ChannelError::Prepare("unsupported OAuth surface".into())),
            }
        })
    }
}

async fn authorize_get(
    ctx: SynthCtx<'_>,
    oauth: &dyn gproxy_channel_api::OAuthService,
) -> Result<SurfaceReply, ChannelError> {
    let request = authorize_query(ctx.query)?;
    validate_authorize(&request)?;
    let Some(user) = oauth.browser_user(ctx.headers).await.map_err(map)? else {
        let return_to = encode(&format!(
            "{}?{}",
            external_path(&ctx),
            ctx.query.unwrap_or_default()
        ));
        return Ok(redirect(&format!("/portal?oauth_return={return_to}")));
    };
    Ok(html(StatusCode::OK, &consent(&request, &user.name)))
}

async fn authorize_post(
    ctx: SynthCtx<'_>,
    provider_id: i64,
    oauth: &dyn gproxy_channel_api::OAuthService,
) -> Result<SurfaceReply, ChannelError> {
    let form: AuthorizeForm = serde_urlencoded::from_bytes(ctx.body)
        .map_err(|_| ChannelError::Prepare("invalid OAuth authorization form".into()))?;
    validate_authorize(&form.query)?;
    let user = oauth
        .browser_user(ctx.headers)
        .await
        .map_err(map)?
        .ok_or_else(|| ChannelError::Prepare("OAuth login required".into()))?;
    if form.decision != "approve" {
        return Ok(callback(
            &form.query.redirect_uri,
            &[("error", "access_denied"), ("state", &form.query.state)],
        ));
    }
    let scopes = scopes(&form.query.scope)?;
    let grant = oauth
        .authorize(
            &user,
            OAuthAuthorizeInput {
                provider_id: Some(provider_id),
                client_id: form.query.client_id.clone(),
                redirect_uri: form.query.redirect_uri.clone(),
                scopes,
                code_challenge: form.query.code_challenge.clone(),
            },
        )
        .await
        .map_err(map)?;
    Ok(callback(
        &form.query.redirect_uri,
        &[("code", &grant.code), ("state", &form.query.state)],
    ))
}

async fn token(
    ctx: SynthCtx<'_>,
    oauth: &dyn gproxy_channel_api::OAuthService,
) -> Result<SurfaceReply, ChannelError> {
    let value = request_value(&ctx)?;
    let grant_type = string(&value, "grant_type")?;
    let client_id = string(&value, "client_id")?;
    if client_id != CLIENT_ID {
        return Ok(oauth_error(StatusCode::BAD_REQUEST, "invalid_client"));
    }
    let issuer = issuer(ctx.headers, ctx.route_name);
    let result = match grant_type {
        "authorization_code" => {
            oauth
                .exchange_code(
                    string(&value, "code")?,
                    client_id,
                    string(&value, "redirect_uri")?,
                    string(&value, "code_verifier")?,
                    &issuer,
                )
                .await
        }
        "refresh_token" => {
            oauth
                .refresh(string(&value, "refresh_token")?, client_id, &issuer)
                .await
        }
        _ => {
            return Ok(oauth_error(
                StatusCode::BAD_REQUEST,
                "unsupported_grant_type",
            ));
        }
    };
    match result {
        Ok(tokens) => Ok(json_reply(
            StatusCode::OK,
            json!({
                "token_type":"Bearer",
                "id_token":tokens.id_token,
                "access_token":tokens.access_token,
                "refresh_token":tokens.refresh_token,
                "expires_in":tokens.expires_in
            }),
        )),
        Err(OAuthError::InvalidGrant) => Ok(oauth_error(StatusCode::BAD_REQUEST, "invalid_grant")),
        Err(error) => Err(map(error)),
    }
}

async fn revoke(
    ctx: SynthCtx<'_>,
    oauth: &dyn gproxy_channel_api::OAuthService,
) -> Result<SurfaceReply, ChannelError> {
    let value = request_value(&ctx)?;
    oauth.revoke(string(&value, "token")?).await.map_err(map)?;
    Ok(json_reply(StatusCode::OK, json!({})))
}

async fn device_start(
    ctx: SynthCtx<'_>,
    provider_id: i64,
    oauth: &dyn gproxy_channel_api::OAuthService,
) -> Result<SurfaceReply, ChannelError> {
    let value = request_value(&ctx)?;
    let client_id = string(&value, "client_id")?;
    if client_id != CLIENT_ID {
        return Ok(oauth_error(StatusCode::BAD_REQUEST, "invalid_client"));
    }
    let issuer = issuer(ctx.headers, ctx.route_name);
    let started = oauth
        .device_start(Some(provider_id), client_id, &issuer)
        .await
        .map_err(map)?;
    Ok(json_reply(
        StatusCode::OK,
        json!({
            "device_auth_id":started.device_auth_id,
            "user_code":started.user_code,
            "interval":started.interval_secs.to_string()
        }),
    ))
}

async fn device_page(
    ctx: SynthCtx<'_>,
    oauth: &dyn gproxy_channel_api::OAuthService,
) -> Result<SurfaceReply, ChannelError> {
    if oauth
        .browser_user(ctx.headers)
        .await
        .map_err(map)?
        .is_none()
    {
        return Ok(redirect(&format!(
            "/portal?oauth_return={}",
            encode(&external_path(&ctx))
        )));
    }
    if ctx.method == http::Method::GET {
        return Ok(html(
            StatusCode::OK,
            "<!doctype html><meta charset=\"utf-8\"><title>Authorize Codex device</title>\
             <main><h1>Authorize Codex device</h1><form method=\"post\">\
             <label>Code <input name=\"user_code\" required autocomplete=\"one-time-code\"></label>\
             <button name=\"decision\" value=\"approve\">Authorize</button></form></main>",
        ));
    }
    let form: DeviceForm = serde_urlencoded::from_bytes(ctx.body)
        .map_err(|_| ChannelError::Prepare("invalid device authorization form".into()))?;
    if form.decision != "approve" {
        return Err(ChannelError::Prepare("device authorization denied".into()));
    }
    let user = oauth
        .browser_user(ctx.headers)
        .await
        .map_err(map)?
        .ok_or_else(|| ChannelError::Prepare("OAuth login required".into()))?;
    let issuer = issuer(ctx.headers, ctx.route_name);
    oauth
        .device_approve(&user, &form.user_code, &issuer)
        .await
        .map_err(map)?;
    Ok(html(
        StatusCode::OK,
        "<h1>Codex device authorized</h1><p>You can return to Codex.</p>",
    ))
}

async fn device_poll(
    ctx: SynthCtx<'_>,
    oauth: &dyn gproxy_channel_api::OAuthService,
) -> Result<SurfaceReply, ChannelError> {
    let value = request_value(&ctx)?;
    match oauth
        .device_poll(
            string(&value, "device_auth_id")?,
            string(&value, "user_code")?,
        )
        .await
        .map_err(map)?
    {
        gproxy_channel_api::OAuthDevicePoll::Pending => Ok(json_reply(
            StatusCode::NOT_FOUND,
            json!({"status":"pending"}),
        )),
        gproxy_channel_api::OAuthDevicePoll::Denied => {
            Ok(oauth_error(StatusCode::FORBIDDEN, "access_denied"))
        }
        gproxy_channel_api::OAuthDevicePoll::Ready {
            authorization_code,
            code_verifier,
            code_challenge,
        } => Ok(json_reply(
            StatusCode::OK,
            json!({
                "authorization_code":authorization_code,
                "code_verifier":code_verifier,
                "code_challenge":code_challenge
            }),
        )),
    }
}

#[derive(Deserialize)]
struct AuthorizeQuery {
    response_type: String,
    client_id: String,
    redirect_uri: String,
    scope: String,
    code_challenge: String,
    code_challenge_method: String,
    state: String,
}

#[derive(Deserialize)]
struct AuthorizeForm {
    #[serde(flatten)]
    query: AuthorizeQuery,
    decision: String,
}

#[derive(Deserialize)]
struct DeviceForm {
    user_code: String,
    decision: String,
}

fn authorize_query(query: Option<&str>) -> Result<AuthorizeQuery, ChannelError> {
    serde_urlencoded::from_str(query.unwrap_or_default())
        .map_err(|_| ChannelError::Prepare("invalid OAuth authorization query".into()))
}

fn validate_authorize(request: &AuthorizeQuery) -> Result<(), ChannelError> {
    if request.response_type != "code"
        || request.client_id != CLIENT_ID
        || request.code_challenge_method != "S256"
        || request.code_challenge.is_empty()
        || request.state.is_empty()
        || !redirect_allowed(&request.redirect_uri)
    {
        return Err(ChannelError::Prepare(
            "invalid OAuth authorization request".into(),
        ));
    }
    scopes(&request.scope).map(|_| ())
}

fn redirect_allowed(value: &str) -> bool {
    value == "http://localhost:1455/auth/callback" || value == "http://localhost:1457/auth/callback"
}

fn scopes(value: &str) -> Result<Vec<String>, ChannelError> {
    let scopes = value.split_ascii_whitespace().collect::<Vec<_>>();
    if scopes.is_empty() || scopes.iter().any(|scope| !SCOPES.contains(scope)) {
        return Err(ChannelError::Prepare("unsupported OAuth scope".into()));
    }
    Ok(scopes.into_iter().map(str::to_owned).collect())
}

fn request_value(ctx: &SynthCtx<'_>) -> Result<Value, ChannelError> {
    let json = ctx
        .headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/json"));
    if json {
        serde_json::from_slice(ctx.body)
            .map_err(|_| ChannelError::Prepare("invalid OAuth JSON".into()))
    } else {
        serde_urlencoded::from_bytes::<std::collections::BTreeMap<String, String>>(ctx.body)
            .map(|values| serde_json::to_value(values).expect("string map serializes"))
            .map_err(|_| ChannelError::Prepare("invalid OAuth form".into()))
    }
}

fn string<'a>(value: &'a Value, name: &str) -> Result<&'a str, ChannelError> {
    value
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ChannelError::Prepare(format!("OAuth field `{name}` missing")))
}

fn consent(request: &AuthorizeQuery, user_name: &str) -> String {
    let hidden = [
        ("response_type", request.response_type.as_str()),
        ("client_id", request.client_id.as_str()),
        ("redirect_uri", request.redirect_uri.as_str()),
        ("scope", request.scope.as_str()),
        ("code_challenge", request.code_challenge.as_str()),
        (
            "code_challenge_method",
            request.code_challenge_method.as_str(),
        ),
        ("state", request.state.as_str()),
    ]
    .into_iter()
    .map(|(name, value)| {
        format!(
            "<input type=\"hidden\" name=\"{}\" value=\"{}\">",
            escape(name),
            escape(value)
        )
    })
    .collect::<String>();
    format!(
        "<!doctype html><meta charset=\"utf-8\"><title>Authorize Codex</title>\
         <main><h1>Authorize Codex</h1><p>Account: {}</p><p>Client: {}</p><p>Scopes: {}</p><form method=\"post\">{}\
         <button name=\"decision\" value=\"approve\">Authorize</button>\
         <button name=\"decision\" value=\"deny\">Deny</button></form></main>",
        escape(user_name),
        escape(&request.client_id),
        escape(&request.scope),
        hidden
    )
}

fn issuer(headers: &HeaderMap, route_name: Option<&str>) -> String {
    let host = headers
        .get(http::header::HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("localhost");
    let local = host == "localhost"
        || host.starts_with("localhost:")
        || host == "127.0.0.1"
        || host.starts_with("127.0.0.1:")
        || host == "[::1]"
        || host.starts_with("[::1]:");
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .filter(|value| matches!(*value, "http" | "https"))
        .unwrap_or(if local { "http" } else { "https" });
    route_name.map_or_else(
        || format!("{scheme}://{host}"),
        |name| format!("{scheme}://{host}/{name}"),
    )
}

fn callback(base: &str, pairs: &[(&str, &str)]) -> SurfaceReply {
    let separator = if base.contains('?') { '&' } else { '?' };
    let query = pairs
        .iter()
        .map(|(name, value)| format!("{}={}", encode(name), encode(value)))
        .collect::<Vec<_>>()
        .join("&");
    redirect(&format!("{base}{separator}{query}"))
}

fn redirect(location: &str) -> SurfaceReply {
    let mut headers = HeaderMap::new();
    headers.insert(
        LOCATION,
        HeaderValue::from_str(location).expect("generated redirect is a valid header"),
    );
    SurfaceReply {
        status: StatusCode::FOUND,
        headers,
        body: SurfaceBody::Full(Bytes::new()),
    }
}

fn html(status: StatusCode, body: &str) -> SurfaceReply {
    let mut headers = HeaderMap::new();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    SurfaceReply {
        status,
        headers,
        body: SurfaceBody::Full(Bytes::from(body.to_owned())),
    }
}

fn json_reply(status: StatusCode, value: Value) -> SurfaceReply {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    SurfaceReply {
        status,
        headers,
        body: SurfaceBody::Full(Bytes::from(
            serde_json::to_vec(&value).expect("JSON value serializes"),
        )),
    }
}

fn oauth_error(status: StatusCode, error: &str) -> SurfaceReply {
    json_reply(status, json!({"error":error}))
}

fn map(error: OAuthError) -> ChannelError {
    ChannelError::Prepare(error.to_string())
}

fn encode(value: &str) -> String {
    form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

fn external_path(ctx: &SynthCtx<'_>) -> String {
    ctx.route_name.map_or_else(
        || ctx.path.to_owned(),
        |name| format!("/{name}{}", ctx.path),
    )
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

use super::{
    OrcaTerm,
    config::{OrcaTermConfig, invalid, new_id, now_ms},
    request,
};
use crate::channels::shared::compatible::http::read_body;
use crate::{
    OutboundClient,
    channel::{
        AcquiredCredential, ChannelError, CookieLogin, CredentialRefresh, CredentialUpdate,
        CredentialView, LoginContext, OAuthCredential, OperationFuture, RefreshContext,
    },
};
use gproxy_protocol::HttpBody;
use http::{HeaderMap, HeaderValue, Method, header};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn cookie_value<'a>(cookie: &'a str, key: &str) -> Option<&'a str> {
    let cookie = cookie.trim();
    let cookie = cookie
        .split_once(':')
        .filter(|(name, _)| name.trim().eq_ignore_ascii_case("cookie"))
        .map_or(cookie, |(_, value)| value.trim());
    cookie
        .split(';')
        .filter_map(|part| part.trim().split_once('='))
        .find_map(|(name, value)| (name == key && !value.trim().is_empty()).then_some(value.trim()))
}

pub(super) fn token(secret: &Value) -> Result<&str, ChannelError> {
    secret
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .or_else(|| cookie_value(secret.get("cookie")?.as_str()?, "ot_session"))
        .ok_or(ChannelError::InvalidCredential)
}

fn skey(secret: &Value) -> Option<&str> {
    secret
        .pointer("/provider_secrets/skey")
        .and_then(Value::as_str)
        .or_else(|| cookie_value(secret.get("cookie")?.as_str()?, "skey"))
}

pub(super) fn fact<'a>(credential: CredentialView<'a>, key: &str) -> Option<&'a str> {
    credential
        .secret
        .get("provider_fields")
        .and_then(|v| v.get(key))
        .or_else(|| credential.metadata.get(key))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
}

pub(super) fn csrf(secret: &Value) -> String {
    let Some(skey) = skey(secret).filter(|s| !s.is_empty()) else {
        return String::new();
    };
    let hash = skey.encode_utf16().fold(5381u32, |hash, ch| {
        hash.wrapping_mul(33).wrapping_add(u32::from(ch))
    });
    (hash & 0x7fff_ffff).to_string()
}

pub(super) fn headers(secret: &Value) -> Result<HeaderMap, ChannelError> {
    let token = token(secret)?;
    // Never forward the caller's cookies or tokens. Only this credential's
    // session and optional CSRF cookie belong on OrcaTerm's account transport.
    if token.contains([';', '\r', '\n']) {
        return Err(ChannelError::InvalidCredential);
    }
    let mut cookie = format!("ot_session={token}");
    if let Some(skey) = skey(secret) {
        if skey.contains([';', '\r', '\n']) {
            return Err(ChannelError::InvalidCredential);
        }
        cookie.push_str(&format!("; skey={skey}"));
    }
    let mut headers = HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|_| ChannelError::InvalidCredential)?,
    );
    headers.insert(
        header::COOKIE,
        HeaderValue::from_str(&cookie).map_err(|_| ChannelError::InvalidCredential)?,
    );
    headers.insert("x-product", HeaderValue::from_static("orcaterm"));
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    Ok(headers)
}

pub(super) async fn rpc(
    client: &dyn OutboundClient,
    config: &OrcaTermConfig,
    secret: &Value,
    action: &str,
    data: Value,
) -> Result<Value, ChannelError> {
    let response = rpc_response(client, config, Some(secret), action, data).await?;
    check_response(action, &response)?;
    Ok(response)
}

/// The unauthenticated OAuth exchange uses the same RPC envelope. Keep its
/// pending error available to the login flow rather than classifying it as a
/// rejected account or requiring an existing token before acquiring one.
pub(super) async fn rpc_response(
    client: &dyn OutboundClient,
    config: &OrcaTermConfig,
    secret: Option<&Value>,
    action: &str,
    data: Value,
) -> Result<Value, ChannelError> {
    let site = config.site_url();
    let mut url = url::Url::parse(&format!("{site}/cgi/api"))
        .map_err(|e| ChannelError::InvalidConfig(e.to_string()))?;
    url.query_pairs_mut().append_pair("action", action);
    // The desktop's account RPCs carry the verified account identifiers in
    // the query as well as the session cookie (app.js, otApiRequest).
    if let Some(secret) = secret {
        for (key, parameter) in [("uin", "uin"), ("owner_uin", "ownerUin")] {
            if let Some(value) = secret
                .get("provider_fields")
                .and_then(|fields| fields.get(key))
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
            {
                url.query_pairs_mut().append_pair(parameter, value);
            }
        }
    }
    let mut headers = request::configured_headers(config)?;
    if let Some(secret) = secret {
        headers.extend(self::headers(secret)?);
    } else {
        headers.remove(header::AUTHORIZATION);
        headers.remove(header::COOKIE);
    }
    headers.insert("x-product", HeaderValue::from_static("orcaterm"));
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
    request::insert(&mut headers, "x-seqid", &new_id()?)?;
    request::insert(
        &mut headers,
        "x-csrfcode",
        &secret.map(csrf).unwrap_or_default(),
    )?;
    request::insert(&mut headers, "x-referer", &format!("{site}/terminal"))?;
    let mut req = http::Request::builder()
        .method(Method::POST)
        .uri(url.as_str())
        .body(HttpBody::Bytes(
            json!({"action":action,"data":data}).to_string().into(),
        ))
        .map_err(|e| ChannelError::InvalidConfig(e.to_string()))?;
    *req.headers_mut() = headers;
    let response = client.send(req).await?;
    let status = response.status;
    let bytes = read_body(response.body).await?;
    // A 5xx or transport failure must not retire the account, even if the
    // response body happens to mention an expired token.
    if !status.is_success() {
        return Err(ChannelError::UpstreamResponse {
            status,
            body: bytes,
        });
    }
    let root: Value = serde_json::from_slice(&bytes).map_err(|e| invalid(e.to_string()))?;
    let response = root
        .pointer("/data/Response")
        .ok_or_else(|| invalid("RPC response has no data.Response"))?;
    Ok(response.clone())
}

pub(super) fn check_response(action: &str, response: &Value) -> Result<(), ChannelError> {
    if let Some(error) = response.get("Error") {
        let code = error["Code"].as_str().unwrap_or("unknown");
        if action == "OAuthRefreshToken"
            && matches!(code, "SESSION_EXPIRED" | "INVALID_REFRESH_TOKEN")
        {
            return Err(ChannelError::RefreshRejected(format!("OrcaTerm {code}")));
        }
        return Err(invalid(format!("{action} failed: {code}")));
    }
    Ok(())
}

pub(super) async fn identity(
    client: &dyn OutboundClient,
    config: &OrcaTermConfig,
    secret: &Value,
) -> Result<BTreeMap<String, Value>, ChannelError> {
    let user = rpc(client, config, secret, "GetUserInfo", json!({})).await?;
    let mut fields = BTreeMap::new();
    for (source, target) in [
        ("userId", "user_id"),
        ("uin", "uin"),
        ("ownerUin", "owner_uin"),
        ("nickName", "nickname"),
    ] {
        let value = match &user[source] {
            Value::String(value) => value.clone(),
            Value::Number(value) => value.to_string(),
            _ => continue,
        };
        if !value.is_empty() {
            fields.insert(target.to_owned(), json!(value));
        }
    }
    if !fields.contains_key("user_id") {
        return Err(invalid("GetUserInfo did not identify the account"));
    }
    Ok(fields)
}

async fn renew(
    client: &dyn OutboundClient,
    config: &OrcaTermConfig,
    secret: &Value,
    fields: BTreeMap<String, Value>,
) -> Result<OAuthCredential, ChannelError> {
    let response = rpc(
        client,
        config,
        secret,
        "OAuthRefreshToken",
        json!({"accessToken":token(secret)?}),
    )
    .await?;
    let mut credential = tokens(&response)?;
    credential.provider_fields = fields;
    credential.provider_secrets = secret
        .get("provider_secrets")
        .cloned()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_else(BTreeMap::new);
    if let Some(skey) = skey(secret) {
        credential
            .provider_secrets
            .insert("skey".into(), json!(skey));
    }
    Ok(credential)
}

pub(super) fn tokens(response: &Value) -> Result<OAuthCredential, ChannelError> {
    let access_token = response["AccessToken"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| invalid("token response has no AccessToken"))?
        .to_owned();
    let seconds = response
        .get("ExpiresIn")
        .and_then(Value::as_i64)
        .filter(|n| *n > 0)
        .unwrap_or(7200);
    Ok(OAuthCredential {
        access_token,
        refresh_token: None,
        id_token: None,
        token_type: Some("Bearer".into()),
        scopes: vec![],
        expires_at_ms: Some(now_ms().saturating_add(seconds.saturating_mul(1000))),
        refresh_expires_at_ms: None,
        provider_fields: BTreeMap::new(),
        provider_secrets: BTreeMap::new(),
    })
}

impl CookieLogin for OrcaTerm {
    fn exchange_cookie<'a>(
        &'a self,
        ctx: LoginContext<'a>,
        cookie: &'a str,
    ) -> OperationFuture<'a, AcquiredCredential> {
        Box::pin(async move {
            let config = OrcaTermConfig::from_view(ctx.provider)?;
            let secret = json!({"cookie":cookie});
            token(&secret)?;
            let fields = identity(ctx.client, &config, &secret).await?;
            Ok(renew(ctx.client, &config, &secret, fields).await?.into())
        })
    }
}

impl CredentialRefresh for OrcaTerm {
    fn supports(&self, credential: &CredentialView<'_>) -> bool {
        token(credential.secret).is_ok()
    }

    fn refresh<'a>(&'a self, ctx: RefreshContext<'a>) -> OperationFuture<'a, CredentialUpdate> {
        Box::pin(async move {
            let config = OrcaTermConfig::from_view(ctx.provider)?;
            let mut fields: BTreeMap<String, Value> = ctx
                .credential
                .secret
                .get("provider_fields")
                .cloned()
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default();
            if let Some(metadata) = ctx.credential.metadata.as_object() {
                for (key, value) in metadata {
                    fields.entry(key.clone()).or_insert_with(|| value.clone());
                }
            }
            let mut updated = renew(ctx.client, &config, ctx.credential.secret, fields).await?;
            if !updated.provider_fields.contains_key("user_id") {
                updated.provider_fields = identity(
                    ctx.client,
                    &config,
                    &json!({"access_token":updated.access_token}),
                )
                .await?;
            }
            Ok(CredentialUpdate {
                expires_at_ms: updated.expires_at_ms,
                secret: serde_json::to_value(updated).map_err(|e| invalid(e.to_string()))?,
            })
        })
    }
}

//! The registered client and the token endpoint.

use std::collections::BTreeMap;

use base64::Engine as _;
use gproxy_client::OutboundClient;
use gproxy_protocol::connection::Bytes;
use http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{discovery::AuthorizationServer, http::send};
use crate::UpstreamError;

/// What dynamic registration settled, plus where its tokens come from. It
/// crosses the browser step in `AuthorizationStart::provider_state` and then
/// lives in `OAuthCredential::provider_secrets`.
pub(super) struct RegisteredClient {
    pub client_id: String,
    client_secret: Option<String>,
    token_endpoint: String,
    token_endpoint_auth_method: String,
}

#[derive(Deserialize)]
struct Registration {
    client_id: String,
    client_secret: Option<String>,
    token_endpoint_auth_method: Option<String>,
}

fn text(map: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    map.get(key).and_then(Value::as_str).map(str::to_owned)
}

impl RegisteredClient {
    pub fn into_map(self, resource: String) -> BTreeMap<String, Value> {
        let mut map = BTreeMap::from([
            ("client_id".to_owned(), Value::String(self.client_id)),
            (
                "token_endpoint".to_owned(),
                Value::String(self.token_endpoint),
            ),
            (
                "token_endpoint_auth_method".to_owned(),
                Value::String(self.token_endpoint_auth_method),
            ),
            ("resource".to_owned(), Value::String(resource)),
        ]);
        if let Some(secret) = self.client_secret {
            map.insert("client_secret".to_owned(), Value::String(secret));
        }
        map
    }

    /// The client and the resource indicator `into_map` stored.
    pub fn from_map(map: &BTreeMap<String, Value>) -> Result<(Self, String), UpstreamError> {
        let read = |key| text(map, key).ok_or(UpstreamError::InvalidCredential);
        Ok((
            Self {
                client_id: read("client_id")?,
                client_secret: text(map, "client_secret"),
                token_endpoint: read("token_endpoint")?,
                token_endpoint_auth_method: read("token_endpoint_auth_method")?,
            },
            read("resource")?,
        ))
    }
}

/// RFC 7591: register this gateway as a public client of `server`.
pub(super) async fn register(
    client: &dyn OutboundClient,
    server: &AuthorizationServer,
    client_name: &str,
    redirect_uri: &str,
) -> Result<RegisteredClient, UpstreamError> {
    let endpoint = server.registration_endpoint.as_deref().ok_or_else(|| {
        UpstreamError::InvalidResponse(format!(
            "authorization server `{}` does not register clients dynamically",
            server.issuer
        ))
    })?;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    let body = json!({
        "client_name": client_name,
        "redirect_uris": [redirect_uri],
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "token_endpoint_auth_method": "none",
    });
    let (status, _, body) = send(
        client,
        Method::POST,
        endpoint,
        headers,
        Some(body.to_string().into_bytes()),
    )
    .await?;
    if !status.is_success() {
        return Err(UpstreamError::UpstreamResponse { status, body });
    }
    let registration: Registration = serde_json::from_slice(&body)
        .map_err(|error| UpstreamError::InvalidResponse(format!("client registration: {error}")))?;
    // RFC 7591 section 2: a client that was given a secret and no method uses Basic.
    let method = registration.token_endpoint_auth_method.unwrap_or_else(|| {
        match registration.client_secret {
            Some(_) => "client_secret_basic",
            None => "none",
        }
        .to_owned()
    });
    Ok(RegisteredClient {
        client_id: registration.client_id,
        client_secret: registration.client_secret,
        token_endpoint: server.token_endpoint.clone(),
        token_endpoint_auth_method: method,
    })
}

#[derive(Deserialize)]
pub(super) struct TokenReply {
    pub access_token: String,
    pub token_type: Option<String>,
    pub expires_in: Option<i64>,
    pub refresh_token: Option<String>,
    pub scope: Option<String>,
}

/// A token endpoint's refusal, with the body it came in.
pub(super) struct Refusal {
    pub status: StatusCode,
    pub body: Bytes,
}

impl Refusal {
    /// The OAuth error when it says the grant or the client is gone for good.
    pub fn definitive(&self) -> Option<String> {
        if !matches!(
            self.status,
            StatusCode::BAD_REQUEST | StatusCode::UNAUTHORIZED
        ) {
            return None;
        }
        let body: Value = serde_json::from_slice(&self.body).ok()?;
        let error = body.get("error")?.as_str()?;
        matches!(
            error,
            "invalid_grant" | "invalid_client" | "unauthorized_client"
        )
        .then(
            || match body.get("error_description").and_then(Value::as_str) {
                Some(description) => format!("{error}: {description}"),
                None => error.to_owned(),
            },
        )
    }
}

fn encode(pairs: &[(&str, &str)]) -> String {
    let mut form = form_urlencoded::Serializer::new(String::new());
    for (name, value) in pairs {
        form.append_pair(name, value);
    }
    form.finish()
}

/// One grant at the token endpoint, authenticated the way the client registered.
pub(super) async fn request(
    http: &dyn OutboundClient,
    client: &RegisteredClient,
    grant: &[(&str, &str)],
) -> Result<Result<TokenReply, Refusal>, UpstreamError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/x-www-form-urlencoded"),
    );
    let mut pairs = grant.to_vec();
    match (
        client.token_endpoint_auth_method.as_str(),
        &client.client_secret,
    ) {
        ("client_secret_basic", Some(secret)) => {
            let escape = |text: &str| -> String {
                form_urlencoded::byte_serialize(text.as_bytes()).collect()
            };
            let pair = format!("{}:{}", escape(&client.client_id), escape(secret));
            let value = format!(
                "Basic {}",
                base64::engine::general_purpose::STANDARD.encode(pair)
            );
            headers.insert(
                header::AUTHORIZATION,
                HeaderValue::from_str(&value).map_err(|_| UpstreamError::InvalidCredential)?,
            );
        }
        (_, secret) => {
            pairs.push(("client_id", &client.client_id));
            if let Some(secret) = secret {
                pairs.push(("client_secret", secret));
            }
        }
    }
    let (status, _, body) = send(
        http,
        Method::POST,
        &client.token_endpoint,
        headers,
        Some(encode(&pairs).into_bytes()),
    )
    .await?;
    if !status.is_success() {
        return Ok(Err(Refusal { status, body }));
    }
    serde_json::from_slice(&body)
        .map(Ok)
        .map_err(|error| UpstreamError::InvalidResponse(format!("token endpoint: {error}")))
}

pub(super) fn scopes(scope: Option<&str>) -> Vec<String> {
    scope
        .map(|scope| scope.split_whitespace().map(str::to_owned).collect())
        .unwrap_or_default()
}

pub(super) fn expiry(expires_in: Option<i64>) -> Option<i64> {
    let now = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| i64::try_from(elapsed.as_millis()).ok())?;
    Some(now.saturating_add(expires_in?.saturating_mul(1000)))
}

use gproxy_upstream::{
    ConfigKey, ConfigKeyKind, CredentialRefresh, CredentialView, LoginMode, OAuthAuthorizationCode,
    ProviderView, UpstreamError, UpstreamKind, protected_resource::ProtectedResourceOAuth,
};
use http::{HeaderMap, HeaderName, HeaderValue, header};
use serde_json::Value;

pub const ID: &str = "mcp";

const OAUTH: ProtectedResourceOAuth = ProtectedResourceOAuth {
    client_name: "gproxy",
    // A loopback address nothing listens on, as CLI clients register: the
    // console asks for the address the browser ended on.
    redirect_uri: "http://127.0.0.1:47821/callback",
};

/// One MCP server per provider: `base_url` is its endpoint, `config.headers`
/// are sent on every request, and a credential is an OAuth sign-in
/// (`oauth`), a pasted secret (`api_key`) or nothing at all (`none`).
pub struct Mcp;

impl UpstreamKind for Mcp {
    fn id(&self) -> &'static str {
        ID
    }

    fn display_name(&self) -> &'static str {
        "MCP server"
    }

    fn login_modes(&self) -> Vec<LoginMode> {
        vec![LoginMode::AuthorizationCode, LoginMode::ApiKey]
    }

    fn credential_kinds(&self) -> Vec<&'static str> {
        vec!["oauth", "api_key", "none"]
    }

    fn config_keys(&self) -> Vec<ConfigKey> {
        vec![
            ConfigKey::required(
                "base_url",
                ConfigKeyKind::String,
                "The server's MCP endpoint.",
            )
            .with_placeholder("https://mcp.example.com/mcp"),
            ConfigKey::optional(
                "headers",
                ConfigKeyKind::HeaderList,
                "Headers sent on every request to the server.",
            ),
            ConfigKey::optional(
                "auth_header",
                ConfigKeyKind::String,
                "The header a pasted secret is sent in; `Authorization: Bearer` when unset.",
            )
            .with_placeholder("X-API-Key"),
        ]
    }

    fn validate_credential_secret(&self, secret: &Value) -> Result<(), UpstreamError> {
        authorization(secret).map(drop)
    }

    fn credential_refresh(&self) -> Option<&dyn CredentialRefresh> {
        Some(&OAUTH)
    }

    fn oauth_authorization_code(&self) -> Option<&dyn OAuthAuthorizationCode> {
        Some(&OAUTH)
    }
}

/// What a credential adds to a request: a bearer token for a sign-in or a
/// pasted secret, nothing for `{}`.
fn authorization(secret: &Value) -> Result<Option<String>, UpstreamError> {
    let token = ["access_token", "api_key"]
        .iter()
        .find_map(|field| secret.get(field));
    match (token, secret) {
        (Some(Value::String(token)), _) if !token.trim().is_empty() => Ok(Some(token.clone())),
        (None, Value::Object(fields)) if fields.is_empty() => Ok(None),
        _ => Err(UpstreamError::InvalidCredential),
    }
}

/// The headers of every request to this provider's server with this
/// credential: `config.headers`, then the credential. A pasted secret goes
/// out as `Authorization: Bearer`, or verbatim in `config.auth_header` when
/// the server reads its key from a header of its own.
pub fn request_headers(
    provider: ProviderView<'_>,
    credential: CredentialView<'_>,
) -> Result<HeaderMap, UpstreamError> {
    let invalid =
        |what: &str| UpstreamError::InvalidConfig(format!("provider `{}`: {what}", provider.id));
    let mut headers = HeaderMap::new();
    if let Some(fixed) = provider.config.get("headers") {
        let fixed = fixed
            .as_object()
            .ok_or_else(|| invalid("`headers` must be an object of header names and values"))?;
        for (name, value) in fixed {
            let name = HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| invalid("`headers` has an invalid header name"))?;
            let value = value
                .as_str()
                .and_then(|value| HeaderValue::from_str(value).ok())
                .ok_or_else(|| invalid("`headers` has an invalid header value"))?;
            headers.insert(name, value);
        }
    }
    let Some(token) = authorization(credential.secret)? else {
        return Ok(headers);
    };
    let own_header = provider
        .config
        .get("auth_header")
        .and_then(Value::as_str)
        .filter(|_| credential.auth_kind == "api_key");
    let (name, value) = match own_header {
        Some(name) => (
            HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| invalid("`auth_header` is not a header name"))?,
            token,
        ),
        None => (header::AUTHORIZATION, format!("Bearer {token}")),
    };
    let mut value = HeaderValue::from_str(&value).map_err(|_| UpstreamError::InvalidCredential)?;
    value.set_sensitive(true);
    headers.insert(name, value);
    Ok(headers)
}

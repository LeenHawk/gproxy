use bytes::Bytes;
use http::{Request, header};
use serde_json::Value;

use crate::channel::ChannelError;

pub const DEFAULT_BASE_URL: &str = "https://api.tasklet.ai";

#[derive(Clone)]
pub struct McpConfig {
    pub url: String,
    pub api_key: String,
}

pub fn session_token(secret: &Value) -> Result<&str, ChannelError> {
    required(secret, "session_token")
}

pub fn workspace_id(secret: &Value) -> Result<&str, ChannelError> {
    required(secret, "workspace_id")
}

pub fn mcp_config(secret: &Value) -> Result<McpConfig, ChannelError> {
    let url = required(secret, "mcp_url")?.trim_end_matches('/');
    let uri = url
        .parse::<http::Uri>()
        .map_err(|_| ChannelError::InvalidCredential("mcp_url must be a valid HTTPS URL".into()))?;
    let host = uri.host().unwrap_or_default();
    if uri.scheme_str() != Some("https") || matches!(host, "localhost" | "127.0.0.1" | "::1") {
        return Err(ChannelError::InvalidCredential(
            "mcp_url must be a public HTTPS URL".into(),
        ));
    }
    Ok(McpConfig {
        url: url.to_owned(),
        api_key: required(secret, "mcp_api_key")?.to_owned(),
    })
}

pub fn apply(req: &mut Request<Bytes>, token: &str, json: bool) -> Result<(), ChannelError> {
    let headers = req.headers_mut();
    headers.insert(header::ACCEPT, http::HeaderValue::from_static("*/*"));
    headers.insert(
        header::AUTHORIZATION,
        http::HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|error| ChannelError::InvalidCredential(error.to_string()))?,
    );
    headers.insert(
        header::ORIGIN,
        http::HeaderValue::from_static("https://tasklet.ai"),
    );
    headers.insert(
        header::REFERER,
        http::HeaderValue::from_static("https://tasklet.ai/"),
    );
    if json {
        headers.insert(
            header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/json"),
        );
    }
    Ok(())
}

fn required<'a>(secret: &'a Value, key: &'static str) -> Result<&'a str, ChannelError> {
    secret
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ChannelError::InvalidCredential(format!("missing {key}")))
}

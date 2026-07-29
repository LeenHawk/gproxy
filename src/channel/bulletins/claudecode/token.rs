use std::sync::Arc;

use bytes::Bytes;
use http::Request;
use serde_json::Value;

use crate::channel::{ChannelError, oauth};
use crate::http::client::UpstreamClient;

pub(super) enum Error {
    Channel(ChannelError),
    Rejected {
        status: http::StatusCode,
        code: Option<String>,
        snippet: String,
    },
}

impl Error {
    pub(super) fn is_invalid_scope(&self) -> bool {
        matches!(
            self,
            Self::Rejected {
                status: http::StatusCode::BAD_REQUEST,
                code: Some(code),
                ..
            } if code == "invalid_scope"
        )
    }

    pub(super) fn into_channel_error(self) -> ChannelError {
        match self {
            Self::Rejected {
                status,
                code: Some(code),
                snippet,
                ..
            } if code == "invalid_grant"
                && matches!(
                    status,
                    http::StatusCode::BAD_REQUEST | http::StatusCode::UNAUTHORIZED
                ) =>
            {
                ChannelError::InvalidCredential(format!(
                    "OAuth grant is invalid or expired: {snippet}"
                ))
            }
            Self::Rejected {
                status, snippet, ..
            } => ChannelError::Build(format!("token endpoint {status}: {snippet}")),
            Self::Channel(error) => error,
        }
    }
}

pub(super) fn request(
    form: &[(&str, &str)],
    extra_headers: &[(&str, &str)],
) -> Result<Request<Bytes>, ChannelError> {
    let body = form
        .iter()
        .map(|(key, value)| {
            format!(
                "{}={}",
                oauth::percent_encode(key),
                oauth::percent_encode(value)
            )
        })
        .collect::<Vec<_>>()
        .join("&");
    let mut request = Request::post(super::auth::TOKEN_URL)
        .header(
            http::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .header(http::header::ACCEPT, "application/json, text/plain, */*")
        .body(Bytes::from(body))
        .map_err(|error| ChannelError::Build(format!("token request build: {error}")))?;
    for (key, value) in extra_headers {
        let name = http::header::HeaderName::from_bytes(key.as_bytes())
            .map_err(|error| ChannelError::Build(format!("token header name: {error}")))?;
        let value = http::header::HeaderValue::from_str(value)
            .map_err(|error| ChannelError::Build(format!("token header value: {error}")))?;
        request.headers_mut().insert(name, value);
    }
    request
        .extensions_mut()
        .insert(super::axios::transport_options(
            std::time::Duration::from_secs(30),
            false,
        ));
    Ok(request)
}

pub(super) async fn post(
    client: &Arc<dyn UpstreamClient>,
    form: &[(&str, &str)],
    extra_headers: &[(&str, &str)],
) -> Result<oauth::TokenResponse, Error> {
    send(
        client,
        request(form, extra_headers).map_err(Error::Channel)?,
    )
    .await
}

async fn send(
    client: &Arc<dyn UpstreamClient>,
    request: Request<Bytes>,
) -> Result<oauth::TokenResponse, Error> {
    let response = client.send(request).await.map_err(|error| {
        tracing::warn!(error = %error, "Claude Code OAuth token request failed");
        Error::Channel(ChannelError::Build(format!(
            "token request failed: {error}"
        )))
    })?;
    let (parts, body) = response.into_parts();
    if !parts.status.is_success() {
        let snippet: String = String::from_utf8_lossy(&body).chars().take(256).collect();
        let code = serde_json::from_slice::<Value>(&body)
            .ok()
            .and_then(|value| {
                let error = value.get("error")?;
                error
                    .as_str()
                    .or_else(|| {
                        error
                            .get("type")
                            .or_else(|| error.get("code"))
                            .and_then(Value::as_str)
                    })
                    .map(str::to_owned)
            });
        tracing::warn!(
            status = %parts.status,
            error_code = code.as_deref().unwrap_or("unknown"),
            "Claude Code OAuth token endpoint rejected request"
        );
        return Err(Error::Rejected {
            status: parts.status,
            code,
            snippet,
        });
    }
    serde_json::from_slice(&body).map_err(|error| {
        tracing::warn!(error = %error, "Claude Code OAuth token response was invalid");
        Error::Channel(ChannelError::Build(format!(
            "token response parse: {error}"
        )))
    })
}

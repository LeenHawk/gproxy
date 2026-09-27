//! Turning a native request into a signed Bedrock request.
//!
//! The Bedrock envelope for Anthropic models (v3
//! `aws_bedrock/messages.rs`): the model moves from the body into the URL,
//! `stream` is implied by the path, `fallbacks` is a gateway extension AWS
//! does not know, and `anthropic_version` names Bedrock's own version. An
//! `anthropic-beta` request header has no place in the Bedrock API, so its
//! values are merged into the body's `anthropic_beta` array and the header is
//! dropped rather than forwarded.

use http::{HeaderMap, HeaderValue, header};
use serde_json::{Value, json};

use super::config::BedrockConfig;
use super::endpoint::{self, Plane};
use super::{ID, sigv4};
use crate::channel::{ChannelError, CredentialView, HeaderAllowlist, ProviderView, forwardable};
use gproxy_protocol::HttpBody;
use gproxy_protocol::connection::Bytes;

/// Headers the channel owns: a client must not be able to present its own
/// signature material, and the Anthropic headers are folded into the body.
const ALSO_DROP: &[&str] = &[
    "x-amz-date",
    "x-amz-content-sha256",
    "x-amz-security-token",
    "anthropic-beta",
    "anthropic-version",
];

/// Everything needed to sign, resolved from the provider and credential.
pub(super) struct Signed {
    method: http::Method,
    url: String,
    headers: HeaderMap,
    body: Bytes,
}

/// Where a resolved request goes and what it carries.
pub(super) struct Target<'a> {
    pub plane: Plane,
    pub url: String,
    pub client_headers: &'a HeaderMap,
    pub body: Bytes,
    /// What the reply is asked to be: JSON, or SSE for a streamed Chat
    /// Completions call. InvokeModel's event-stream framing is chosen by the
    /// path, not by this.
    pub accept: &'static str,
}

/// The Bedrock credential: either an access key pair (optionally temporary)
/// or a Bedrock API key, which AWS issues as a plain bearer token.
pub(super) enum Auth<'a> {
    Bearer(&'a str),
    AccessKey(sigv4::Credentials<'a>),
}

impl<'a> Auth<'a> {
    pub(super) fn from_credential(credential: CredentialView<'a>) -> Result<Self, ChannelError> {
        if let Some(key) = field(credential.secret, "api_key") {
            return Ok(Self::Bearer(key));
        }
        let (Some(access_key_id), Some(secret_access_key)) = (
            field(credential.secret, "access_key_id"),
            field(credential.secret, "secret_access_key"),
        ) else {
            return Err(ChannelError::InvalidCredential);
        };
        Ok(Self::AccessKey(sigv4::Credentials {
            access_key_id,
            secret_access_key,
            session_token: field(credential.secret, "session_token"),
        }))
    }
}

fn field<'a>(secret: &'a Value, name: &str) -> Option<&'a str> {
    secret
        .get(name)?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// Build and sign one request. `now_secs` is threaded in so callers (and
/// tests) own the clock.
pub(super) fn build(
    provider: ProviderView<'_>,
    credential: CredentialView<'_>,
    config: &BedrockConfig,
    target: Target<'_>,
    now_secs: u64,
) -> Result<Signed, ChannelError> {
    let uri: http::Uri = target.url.parse().map_err(|error| {
        ChannelError::InvalidConfig(format!("AWS endpoint `{}`: {error}", target.url))
    })?;
    let allowlist = HeaderAllowlist::from_view(provider)?;
    let mut headers = forwardable(target.client_headers, allowlist.as_ref(), ALSO_DROP);
    // Bedrock reads Content-Type as the MIME type of the inference payload and
    // Accept as the MIME type it should answer with; the event-stream framing
    // of the streaming endpoint is not chosen here.
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(header::ACCEPT, HeaderValue::from_static(target.accept));
    let method = endpoint::method_for(target.plane);
    match Auth::from_credential(credential)? {
        Auth::Bearer(key) => {
            headers.insert(
                header::AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {key}"))
                    .map_err(|_| ChannelError::InvalidCredential)?,
            );
        }
        Auth::AccessKey(credentials) => sigv4::sign(
            sigv4::SigningRequest {
                method: &method,
                uri: &uri,
                payload: &target.body,
            },
            &mut headers,
            credentials,
            sigv4::Scope {
                region: config.region()?,
                service: endpoint::SIGNING_SERVICE,
            },
            now_secs,
        )?,
    }
    Ok(Signed {
        method,
        url: target.url,
        headers,
        body: target.body,
    })
}

impl Signed {
    pub(super) fn into_request(self) -> Result<http::Request<HttpBody>, ChannelError> {
        let mut builder = http::Request::builder().method(self.method).uri(self.url);
        if let Some(map) = builder.headers_mut() {
            *map = self.headers;
        }
        builder
            .body(HttpBody::Bytes(self.body))
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }
}

/// The Claude Messages body rewritten into Bedrock's InvokeModel envelope.
pub(super) fn invoke_body(
    body: &[u8],
    config: &BedrockConfig,
    client_headers: &HeaderMap,
) -> Result<Bytes, ChannelError> {
    let mut value: Value = serde_json::from_slice(body).map_err(|error| {
        ChannelError::InvalidConfig(format!("{ID} request body is not JSON: {error}"))
    })?;
    let object = value.as_object_mut().ok_or_else(|| {
        ChannelError::InvalidConfig(format!("{ID} request body is not an object"))
    })?;
    object.remove("model");
    object.remove("stream");
    object.remove("fallbacks");
    object.insert(
        "anthropic_version".into(),
        Value::String(config.anthropic_version.clone()),
    );
    let mut betas = object
        .get("anthropic_beta")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for beta in client_headers
        .get("anthropic-beta")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|beta| !beta.is_empty())
    {
        if !betas.iter().any(|value| value.as_str() == Some(beta)) {
            betas.push(json!(beta));
        }
    }
    if betas.is_empty() {
        object.remove("anthropic_beta");
    } else {
        object.insert("anthropic_beta".into(), Value::Array(betas));
    }
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
}

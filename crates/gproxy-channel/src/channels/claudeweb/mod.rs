//! Claude Web: a claude.ai browser session used as an upstream.
//!
//! Wire facts follow the v3 channel (`crates/gproxy-channels/src/claudeweb`
//! on `main`), cross-checked against `samples/clewdr`: the session is a
//! `sessionKey` cookie validated at `/api/bootstrap`; a generation is a
//! temporary conversation under `/api/organizations/{org}/chat_conversations`
//! whose `/completion` streams Messages-shaped events; tool results go to
//! `/tool_result` on the same conversation; account limits come from
//! `/api/organizations/{org}/usage`.
//!
//! Unlike v3, no host orchestration is involved: `generate_content` and
//! `stream_generate_content` run every step themselves with the owned client
//! and record tool boundaries in the scoped `ChannelState` (see `turn`).
//!
//! The TLS/HTTP fingerprint claude.ai expects from a browser is a
//! ConnectionProfile concern: the host binds this provider to a client with
//! browser emulation (wreq) and the channel only supplies the headers the
//! front end sends. Client-supplied headers pass through `forwardable`
//! minus the session's own identity headers and are further narrowed by
//! `config.allowed_headers`; `user-agent` is never forwarded because the
//! emulation profile owns it.

mod auth;
mod bootstrap;
mod id;
mod login;
mod models;
mod prepare;
mod quota;
mod request;
mod stream;
mod turn;
mod usage;

pub use auth::{DEFAULT_BASE_URL, VALIDATION_SECS};
pub use quota::{FIVE_HOUR_ID, SEVEN_DAY_ID};
pub use stream::SessionState;
pub use turn::{CONTINUATION_SECS, Registry, continuation_key};

use std::{collections::BTreeMap, sync::Arc};

use futures_util::StreamExt;
use gproxy_protocol::{
    Dialect, HttpBody, Operation, WireResponse,
    capability::{CapabilityError, CapabilityErrorKind, CapabilityErrorStage},
    connection::Bytes,
};
use http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use serde::Deserialize;

use crate::channel::{
    BaseChannel, ChannelError, CookieLogin, CredentialRefresh, OperationContext, OperationFuture,
    ProviderView, QuotaModel, QuotaQuery, UsageExtractor, UsageStream,
};

pub const ID: &str = "claudeweb";
/// Bootstrap replies carry the whole front-end configuration.
const MAX_SERVICE_BODY: usize = 16 * 1024 * 1024;
/// Client requests may embed base64 images destined for upload.
const MAX_REQUEST_BODY: usize = 64 * 1024 * 1024;

/// Provider `config` JSON understood by this channel. Unknown keys are ignored.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct ClaudeWebConfig {
    /// Text placed before the flattened conversation in every prompt.
    pub prompt: String,
    /// The `timezone` the web request declares.
    pub timezone: String,
    /// Per-step URL overrides keyed `claudeweb_upload`,
    /// `claudeweb_conversation_create`, `claudeweb_conversation_settings`,
    /// `claudeweb_completion`, `claudeweb_tool_result`, `claudeweb_cleanup`,
    /// `claudeweb_bootstrap`, `claudeweb_usage`; `{organization}` and
    /// `{conversation}` are substituted.
    pub endpoints: BTreeMap<String, String>,
    /// Static headers added to every claude.ai request.
    pub headers: BTreeMap<String, String>,
}

impl Default for ClaudeWebConfig {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            timezone: "UTC".into(),
            endpoints: BTreeMap::new(),
            headers: BTreeMap::new(),
        }
    }
}

impl ClaudeWebConfig {
    pub fn from_view(provider: ProviderView<'_>) -> Result<Self, ChannelError> {
        serde_json::from_value(provider.config.clone())
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }

    fn static_headers(&self, headers: &mut HeaderMap) -> Result<(), ChannelError> {
        for (name, value) in &self.headers {
            headers.insert(
                HeaderName::from_bytes(name.as_bytes())
                    .map_err(|_| ChannelError::InvalidConfig(format!("header `{name}`")))?,
                HeaderValue::from_str(value)
                    .map_err(|_| ChannelError::InvalidConfig(format!("header `{name}`")))?,
            );
        }
        Ok(())
    }
}

/// The channel plus the registry of completion connections parked at a
/// tool boundary; register one instance per process.
#[derive(Default)]
pub struct ClaudeWeb {
    registry: Arc<Registry>,
}

impl ClaudeWeb {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }
}

fn bad_request(message: impl Into<String>) -> ChannelError {
    ChannelError::Transport(CapabilityError::new(
        CapabilityErrorKind::Invalid,
        CapabilityErrorStage::Start,
        message,
    ))
}

fn expired(message: impl Into<String>) -> ChannelError {
    ChannelError::Transport(CapabilityError::new(
        CapabilityErrorKind::Expired,
        CapabilityErrorStage::Start,
        message,
    ))
}

fn now_ms() -> Result<i64, ChannelError> {
    let millis = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_err(|_| ChannelError::InvalidConfig("system clock is before the Unix epoch".into()))?
        .as_millis();
    i64::try_from(millis)
        .map_err(|_| ChannelError::InvalidConfig("Unix milliseconds overflow".into()))
}

/// A wall-clock instant for `StateWrite::expires_at`; built from the epoch
/// so it never calls `std::time::SystemTime::now`, which wasm lacks.
fn system_time(unix_ms: i64) -> std::time::SystemTime {
    std::time::UNIX_EPOCH + std::time::Duration::from_millis(unix_ms.max(0).unsigned_abs())
}

async fn read_body(body: HttpBody, limit: usize) -> Result<Bytes, ChannelError> {
    match body {
        HttpBody::Bytes(bytes) => Ok(bytes),
        HttpBody::Stream(mut stream) => {
            let mut out = Vec::new();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|e| ChannelError::InvalidResponse(e.to_string()))?;
                out.extend_from_slice(&chunk);
                if out.len() > limit {
                    return Err(ChannelError::InvalidResponse(
                        "body exceeds the read limit".into(),
                    ));
                }
            }
            Ok(Bytes::from(out))
        }
    }
}

fn local_response(content_type: &'static str, body: Bytes) -> WireResponse<HttpBody> {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    WireResponse {
        status: StatusCode::OK,
        headers,
        body: HttpBody::Bytes(body),
    }
}

impl BaseChannel for ClaudeWeb {
    fn id(&self) -> &'static str {
        ID
    }

    fn native_dialects(&self, _provider: ProviderView<'_>, operation: Operation) -> Vec<Dialect> {
        match operation {
            Operation::GenerateContent
            | Operation::StreamGenerateContent
            | Operation::ListModels => vec![Dialect::Claude],
            _ => Vec::new(),
        }
    }

    /// Answered locally from the model list recorded at login.
    fn list_models<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(async move {
            if context.dialect != Dialect::Claude {
                return Err(ChannelError::UnsupportedOperation(
                    gproxy_protocol::OperationKey {
                        operation: Operation::ListModels,
                        dialect: context.dialect,
                    },
                ));
            }
            let models = models::from_metadata(context.credential.metadata);
            let body = serde_json::to_vec(&models::claude_list(&models))
                .map_err(|error| ChannelError::InvalidResponse(error.to_string()))?;
            Ok(local_response("application/json", Bytes::from(body)))
        })
    }

    fn generate_content<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(async move {
            if context.dialect != Dialect::Claude {
                return Err(ChannelError::UnsupportedOperation(
                    gproxy_protocol::OperationKey {
                        operation: Operation::GenerateContent,
                        dialect: context.dialect,
                    },
                ));
            }
            let turn = turn::start(self, context).await?;
            let message = turn.collect().await?;
            let body = serde_json::to_vec(&message)
                .map_err(|error| ChannelError::InvalidResponse(error.to_string()))?;
            Ok(local_response("application/json", Bytes::from(body)))
        })
    }

    fn stream_generate_content<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(async move {
            if context.dialect != Dialect::Claude {
                return Err(ChannelError::UnsupportedOperation(
                    gproxy_protocol::OperationKey {
                        operation: Operation::StreamGenerateContent,
                        dialect: context.dialect,
                    },
                ));
            }
            let turn = turn::start(self, context).await?;
            let mut headers = HeaderMap::new();
            headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/event-stream"),
            );
            headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
            Ok(WireResponse {
                status: StatusCode::OK,
                headers,
                body: HttpBody::Stream(turn.into_stream()),
            })
        })
    }

    fn credential_refresh(&self) -> Option<&dyn CredentialRefresh> {
        Some(self)
    }
    fn cookie_login(&self) -> Option<&dyn CookieLogin> {
        Some(self)
    }
    fn quota_query(&self) -> Option<&dyn QuotaQuery> {
        Some(self)
    }
    fn quota_model(&self) -> Option<&dyn QuotaModel> {
        Some(self)
    }
    fn usage_extractor(&self) -> Option<&dyn UsageExtractor> {
        Some(self)
    }
    fn usage_stream(&self) -> Option<&dyn UsageStream> {
        Some(self)
    }
}

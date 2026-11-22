//! One request, from the access key to the answer.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use futures_util::StreamExt;
use gproxy_channel::BaseChannel;
use gproxy_client::OutboundClient;
use gproxy_convert::{surface, upstream_error};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, WireRequest, WireResponse,
    capability::{CapabilityError, CapabilityErrorKind},
    codec::CodecLimits,
    connection::Bytes,
    transform::{TransformError, TransformErrorKind},
};
use gproxy_upstream::UpstreamError;
use http::{Method, StatusCode};
use serde_json::{Value, json};

use crate::{
    ConfigError, KeyValue, MAX_REQUEST_BYTES, auth,
    config::{Config, Provider},
    store::Store,
};

pub(crate) const CODEC: CodecLimits = CodecLimits {
    max_buffer_bytes: 16 * 1024 * 1024,
    max_value_bytes: 16 * 1024 * 1024,
    max_body_bytes: MAX_REQUEST_BYTES,
    max_line_bytes: 16 * 1024 * 1024,
    max_part_bytes: MAX_REQUEST_BYTES,
    max_parts: 64,
};

pub(crate) struct Inner {
    pub access_key: String,
    pub providers: Vec<Provider>,
    pub store: Store,
    pub client: Arc<dyn OutboundClient>,
    /// Per provider: the credential version whose renewal the upstream
    /// refused, where the channel keeps such a refusal out of the store.
    pub rejections: Mutex<HashMap<usize, (i64, String)>>,
}

/// A request the engine answers itself, with the reason.
#[derive(Debug)]
pub(crate) struct Refusal {
    pub status: StatusCode,
    pub message: String,
}

impl Refusal {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    /// The error body of the dialect the client spoke, OpenAI's when the
    /// request never got as far as naming one.
    pub fn response(self, dialect: Option<Dialect>) -> WireResponse<HttpBody> {
        // `503` is this engine's own (no credential), not an overloaded upstream.
        let kind = match self.status {
            StatusCode::SERVICE_UNAVAILABLE => "api_error",
            status => upstream_error::kind(status.as_u16()),
        };
        let detail = json!({ "message": self.message, "type": kind });
        let body = dialect
            .and_then(|dialect| upstream_error::body(&detail, self.status.as_u16(), dialect))
            .or_else(|| upstream_error::body(&detail, self.status.as_u16(), Dialect::OpenAi))
            .expect("OpenAI has an error body");
        json_response(self.status, &body)
    }
}

impl From<CapabilityError> for Refusal {
    fn from(error: CapabilityError) -> Self {
        let status = match error.kind() {
            CapabilityErrorKind::Unsupported => StatusCode::NOT_IMPLEMENTED,
            CapabilityErrorKind::Invalid => StatusCode::BAD_REQUEST,
            CapabilityErrorKind::Limit => StatusCode::PAYLOAD_TOO_LARGE,
            _ => StatusCode::BAD_GATEWAY,
        };
        Self::new(status, error.to_string())
    }
}

impl From<UpstreamError> for Refusal {
    fn from(error: UpstreamError) -> Self {
        match error {
            UpstreamError::Transport(error) => error.into(),
            UpstreamError::UnsupportedOperation(_) | UpstreamError::WrongTransport(_) => {
                Self::new(StatusCode::NOT_IMPLEMENTED, error.to_string())
            }
            UpstreamError::InvalidConfig(_) => {
                Self::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
            }
            UpstreamError::InvalidCredential | UpstreamError::RefreshRejected(_) => {
                Self::new(StatusCode::SERVICE_UNAVAILABLE, error.to_string())
            }
            other => Self::new(StatusCode::BAD_GATEWAY, other.to_string()),
        }
    }
}

impl From<TransformError> for Refusal {
    fn from(error: TransformError) -> Self {
        let status = match error.kind() {
            TransformErrorKind::InvalidInput
            | TransformErrorKind::MissingMetadata
            | TransformErrorKind::MissingState => StatusCode::BAD_REQUEST,
            TransformErrorKind::Unsupported => StatusCode::NOT_IMPLEMENTED,
            TransformErrorKind::Conflict => StatusCode::CONFLICT,
            TransformErrorKind::Limit => StatusCode::PAYLOAD_TOO_LARGE,
            _ => StatusCode::BAD_GATEWAY,
        };
        Self::new(status, error.to_string())
    }
}

pub(crate) fn json_response(status: StatusCode, body: &Value) -> WireResponse<HttpBody> {
    WireResponse {
        status,
        headers: http::HeaderMap::from_iter([(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/json"),
        )]),
        body: HttpBody::Bytes(Bytes::from(body.to_string())),
    }
}

/// The request body, whole. Everything the engine does with a request reads
/// it, so it is buffered once, up to [`MAX_REQUEST_BYTES`].
pub(crate) async fn buffer(body: HttpBody) -> Result<Bytes, Refusal> {
    let too_large = || Refusal::new(StatusCode::PAYLOAD_TOO_LARGE, "request body too large");
    match body {
        HttpBody::Bytes(bytes) if bytes.len() as u64 > MAX_REQUEST_BYTES => Err(too_large()),
        HttpBody::Bytes(bytes) => Ok(bytes),
        HttpBody::Stream(mut stream) => {
            let mut collected = Vec::new();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|error| Refusal::bad_request(error.to_string()))?;
                if (collected.len() + chunk.len()) as u64 > MAX_REQUEST_BYTES {
                    return Err(too_large());
                }
                collected.extend_from_slice(&chunk);
            }
            Ok(Bytes::from(collected))
        }
    }
}

/// What a request addressed: the provider, and the model as that provider's
/// upstream knows it.
pub(crate) struct Addressed {
    pub provider: usize,
    pub model: Option<String>,
}

impl Inner {
    /// `/{provider}/…` names a provider when what follows is a path this
    /// engine serves; anything else is left whole.
    pub(crate) fn mount<'p>(&self, method: &Method, path: &'p str) -> (Option<usize>, &'p str) {
        let Some((name, remainder)) = path
            .strip_prefix('/')
            .and_then(|rest| rest.find('/').map(|at| rest.split_at(at)))
        else {
            return (None, path);
        };
        match self.providers.iter().position(|p| p.name == name) {
            Some(provider)
                if surface::is_surface(method, remainder)
                    || (self.providers[provider].channel.id() == "codex"
                        && surface::match_codex_path(
                            method,
                            remainder,
                            &http::HeaderMap::new(),
                            None,
                        )
                        .is_some()) =>
            {
                (Some(provider), remainder)
            }
            _ => (None, path),
        }
    }

    /// On a provider path the model is the upstream's own name. Elsewhere the
    /// model field says `provider/model`; with one provider configured the
    /// prefix may be left out.
    pub(crate) fn address(
        &self,
        mount: Option<usize>,
        named: Option<String>,
    ) -> Result<Addressed, Refusal> {
        if let Some(provider) = mount {
            return Ok(Addressed {
                provider,
                model: named,
            });
        }
        let prefixed = named.as_deref().and_then(|model| {
            let (name, rest) = model.split_once('/')?;
            let provider = self.providers.iter().position(|p| p.name == name)?;
            Some(Addressed {
                provider,
                model: Some(rest.to_owned()),
            })
        });
        match prefixed {
            Some(addressed) => Ok(addressed),
            None if self.providers.len() == 1 => Ok(Addressed {
                provider: 0,
                model: named,
            }),
            None => Err(Refusal::bad_request(
                "name the provider: `provider/model` in the model field, or a `/{provider}/…` path",
            )),
        }
    }

    async fn serve(
        self: &Arc<Self>,
        request: http::Request<HttpBody>,
        dialect: &mut Option<Dialect>,
    ) -> Result<WireResponse<HttpBody>, Refusal> {
        let (parts, body) = request.into_parts();
        let path = parts.uri.path();
        let query = parts.uri.query();
        if !auth::presented(&parts.headers, query)
            .is_some_and(|key| auth::matches(&key, &self.access_key))
        {
            return Err(Refusal::new(
                StatusCode::UNAUTHORIZED,
                "the access key is missing or wrong",
            ));
        }
        let body = buffer(body).await?;
        if let Some(action) = path.strip_prefix("/lite/") {
            return crate::login::serve(self, &parts.method, action, &body).await;
        }
        let (mount, remainder) = self.mount(&parts.method, path);
        let json: Option<Value> = (!body.is_empty())
            .then(|| serde_json::from_slice(&body).ok())
            .flatten();
        let codex = mount.is_some_and(|provider| self.providers[provider].channel.id() == "codex");
        let matched = if codex {
            surface::match_codex_path(&parts.method, remainder, &parts.headers, json.as_ref())
        } else {
            surface::match_path(&parts.method, remainder, &parts.headers, json.as_ref())
        }
        .ok_or_else(|| Refusal::new(StatusCode::NOT_FOUND, "no such path"))?;
        *dialect = Some(matched.operation.dialect);
        if matched.upgrade {
            return Err(Refusal::new(
                StatusCode::UPGRADE_REQUIRED,
                "this path is a WebSocket",
            ));
        }
        let named = match matched.model {
            Some(model) => Some(model),
            None if matched.operation.dialect == Dialect::GeminiInteractions => json
                .as_ref()
                .and_then(gproxy_protocol::gemini::interactions::request_target)
                .map(str::to_owned),
            None if matched.operation.operation == Operation::CreateVoice => None,
            None => match &json {
                Some(json) => json.get("model").and_then(Value::as_str).map(str::to_owned),
                None => gproxy_convert::form::model(&parts.headers, &body, CODEC)
                    .await
                    .map_err(|error| Refusal::bad_request(error.to_string()))?,
            },
        };
        if mount.is_none()
            && self.providers.len() > 1
            && matched.operation.operation == Operation::ListModels
        {
            return crate::models::every_provider(self, matched.operation, &parts.headers).await;
        }
        let addressed = self.address(mount, named)?;
        let path = match &addressed.model {
            // The path named `provider/model`; the upstream is asked for its own.
            Some(model)
                if mount.is_none() && matched.operation.operation == Operation::GetModel =>
            {
                gproxy_convert::get_model_path(matched.operation.dialect, model)?
            }
            _ if codex => surface::codex_path(remainder).into_owned(),
            _ => remainder.to_owned(),
        };
        let wire = WireRequest {
            method: parts.method,
            path,
            query: query.map(str::to_owned),
            headers: parts.headers,
            body: HttpBody::Bytes(body),
        };
        crate::exec::run(self, addressed, matched.operation, wire).await
    }
}

/// The thin engine: one access key, the configured providers, and whatever
/// the host's key-value store remembers.
#[derive(Clone)]
pub struct Engine(pub(crate) Arc<Inner>);

impl Engine {
    /// `config` is the JSON document described in the crate documentation.
    /// `channels` are the channels this build offers, normally
    /// `gproxy_channel::channels::compiled_in()`.
    pub fn new(
        config: &str,
        channels: &[Arc<dyn BaseChannel>],
        state: Arc<dyn KeyValue>,
        client: Arc<dyn OutboundClient>,
    ) -> Result<Self, ConfigError> {
        let config = Config::parse(config, channels)?;
        Ok(Self(Arc::new(Inner {
            access_key: config.access_key,
            providers: config.providers,
            store: Store::new(state),
            client,
            rejections: Mutex::default(),
        })))
    }

    /// Answer one HTTP request. Every outcome is a response: a refusal is an
    /// error body in the dialect the client spoke.
    pub async fn handle(&self, request: http::Request<HttpBody>) -> WireResponse<HttpBody> {
        let mut dialect = None;
        match self.0.serve(request, &mut dialect).await {
            Ok(response) => response,
            Err(refusal) => refusal.response(dialect),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemoryState;

    struct NoClient;
    impl OutboundClient for NoClient {
        fn send<'a>(
            &'a self,
            _request: http::Request<HttpBody>,
        ) -> gproxy_protocol::capability::CapabilityFuture<
            'a,
            Result<WireResponse<HttpBody>, CapabilityError>,
        > {
            unreachable!("addressing sends nothing")
        }
    }

    fn engine(providers: &[&str]) -> Engine {
        let providers: Vec<Value> = providers
            .iter()
            .map(|name| json!({"name": name, "channel": "custom", "credential": "k"}))
            .collect();
        Engine::new(
            &json!({"access_key": "k", "providers": providers}).to_string(),
            &gproxy_channel::channels::compiled_in(),
            Arc::new(MemoryState::default()),
            Arc::new(NoClient),
        )
        .unwrap_or_else(|error| panic!("{error}"))
    }

    fn addressed(engine: &Engine, path: &str, model: Option<&str>) -> Option<(usize, String)> {
        let (mount, remainder) = engine.0.mount(&Method::POST, path);
        assert!(surface::is_surface(&Method::POST, remainder), "{remainder}");
        let addressed = engine.0.address(mount, model.map(str::to_owned)).ok()?;
        Some((addressed.provider, addressed.model.unwrap_or_default()))
    }

    #[test]
    fn a_request_names_its_provider_in_the_path_or_in_the_model() {
        let engine = engine(&["a", "b"]);
        let chat = "/v1/chat/completions";
        // A provider path: the model is the upstream's, slashes and all.
        assert_eq!(
            addressed(&engine, "/b/v1/chat/completions", Some("a/m")),
            Some((1, "a/m".into()))
        );
        // `provider/model`: only the provider's own prefix is removed.
        assert_eq!(
            addressed(&engine, chat, Some("b/org/m")),
            Some((1, "org/m".into()))
        );
        // Neither, with two providers to choose from.
        assert_eq!(addressed(&engine, chat, Some("m")), None);
        assert_eq!(addressed(&engine, chat, Some("org/m")), None);
        assert_eq!(addressed(&engine, chat, None), None);
        // A first segment that is not followed by a served path is no mount.
        let (mount, remainder) = engine.0.mount(&Method::POST, "/a/nothing");
        assert_eq!((mount, remainder), (None, "/a/nothing"));
        let (mount, _) = engine.0.mount(&Method::GET, "/a/v1/chat/completions");
        assert_eq!(mount, None);
    }

    #[test]
    fn a_single_provider_needs_no_prefix() {
        let engine = engine(&["only"]);
        let chat = "/v1/chat/completions";
        assert_eq!(
            addressed(&engine, chat, Some("org/m")),
            Some((0, "org/m".into()))
        );
        assert_eq!(
            addressed(&engine, chat, Some("only/m")),
            Some((0, "m".into()))
        );
        assert_eq!(addressed(&engine, chat, None), Some((0, String::new())));
    }
}

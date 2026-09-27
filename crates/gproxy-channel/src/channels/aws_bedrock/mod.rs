//! AWS Bedrock: an AWS account used as an upstream, where every request is
//! signed rather than bearing a token.
//!
//! Wire facts follow the v3 channel (`crates/gproxy-channels/src/aws_bedrock`
//! on `main`) and AWS's Bedrock API reference.
//!
//! # Authentication
//!
//! A credential is either an AWS access key pair — `access_key_id` plus
//! `secret_access_key`, with `session_token` when it is a temporary one — or
//! a Bedrock API key, which AWS issues as a plain bearer token. A key pair is
//! signed with SigV4 on every request (`sigv4`); an API key becomes
//! `Authorization: Bearer`. There is no refresh: v3 never assumed a role and
//! neither does this channel, so a temporary credential is stored as AWS
//! minted it and the host expires it by `expires_at_ms`. Minting one needs an
//! STS `AssumeRole` call, which `prepare` may not make; adding it later means
//! adding a `CredentialRefresh`, not changing the signing path.
//!
//! # Dialects per model family
//!
//! Bedrock has no single request shape; the wire follows the model
//! (`native_dialects_for_model`, `endpoint::serves_messages`):
//!
//! - **Anthropic models** (`anthropic.*`, the `us.`/`eu.`/`apac.`/`global.`
//!   inference profiles over them, and ARNs naming either) take Claude
//!   Messages on `InvokeModel` inside a Bedrock envelope: `model` and
//!   `stream` move out of the body into the URL and `anthropic_version`
//!   names Bedrock's own version, `bedrock-2023-05-31`. They do not take
//!   Chat Completions on Bedrock, so an OpenAI client's request to one is
//!   converted to Messages first.
//! - **Everything else** is sent to Bedrock's OpenAI-compatible Chat
//!   Completions on the runtime plane (`/openai/v1/chat/completions`), with
//!   the model in the body and SSE for a streamed reply. That is the only
//!   wire GPT-5.x and Grok 4.3 have on Bedrock, and the one Qwen, DeepSeek,
//!   GLM, Kimi, MiniMax and recent Mistral models share.
//! - An application inference profile's ARN names no family and stays on
//!   Messages, the wire this channel served before it could tell.
//! - Families with neither Messages nor Chat Completions (Llama, Nova,
//!   Jamba, Command, older Mistral) need the `Converse` API, whose shape is
//!   neither; v4's `Dialect` has no Converse variant, so those models are not
//!   served yet.
//!
//! Model discovery is the control plane's `/foundation-models`, rewritten
//! into an OpenAI list (`models`), so `native_dialects` answers
//! `[Dialect::OpenAi]` for `ListModels` and `GetModel`.
//!
//! # Streaming
//!
//! `InvokeModelWithResponseStream` answers with AWS event-stream frames, not
//! SSE; the Messages events are base64 inside each frame's `bytes` field.
//! `stream_generate_content` is therefore overridden to translate the framing
//! (`stream`), the same shape `claudeweb` uses. The buffered `invoke` reply
//! needs no rewriting: for an Anthropic model it is already the Messages
//! response body.
//!
//! # Not ported from v3
//!
//! - the Converse and converse-stream shapes, as above;
//! - `CountTokens`: Bedrock's `/model/{id}/count-tokens` wraps either an
//!   InvokeModel or a Converse body in an `input` envelope and v3's shape for
//!   it could not be confirmed, so it waits for the Converse work;
//! - the `/async-invoke` video jobs, which relied on v3's `ResourceMutation`
//!   and ARN bookkeeping;
//! - quota: v3 read Service Quotas (`servicequotas.{region}.amazonaws.com`,
//!   signed as the `servicequotas` service). `sigv4::Scope` already takes the
//!   service name, so a `QuotaQuery` can be added without touching signing.
//!
//! v3's `SurfaceTable`/`route!` table becomes `native_dialects`, its
//! `StreamDecoder` becomes the `stream_generate_content` override, whose
//! Messages SSE is what the host meters, and it had no `ChannelLogin`: an access key is pasted, not
//! granted.

pub mod sigv4;

mod config;
mod endpoint;
mod models;
mod request;
mod stream;

pub use config::{BedrockConfig, DEFAULT_ANTHROPIC_VERSION, DEFAULT_REGION};
pub use endpoint::SIGNING_SERVICE;

use futures_util::StreamExt as _;
use gproxy_protocol::connection::{ByteStream, Bytes};
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireResponse};
use http::{HeaderMap, HeaderValue, StatusCode, header};

use crate::channel::{
    BaseChannel, ChannelCapabilities, ChannelDescriptor, ChannelError, ConfigKey, ConfigKeyKind,
    CredentialView, HOST_CONFIG_KEYS, LoginMode, OperationContext, OperationFuture, PrepareContext,
    ProviderView, };
use crate::channels::shared::cache;
use endpoint::Plane;

pub const ID: &str = "aws_bedrock";

/// A foundation-model directory page is a few hundred kilobytes at most.
const MAX_MODEL_BODY: usize = 8 * 1024 * 1024;

const STREAMED_BODY: &str =
    "AWS SigV4 signs the payload hash, so a streamed request body cannot be sent to Bedrock";

#[derive(Debug, Default, Clone, Copy)]
pub struct AwsBedrock;

/// Everything the signing path reads out of a request, in one place so no
/// function here grows an unreadable argument list.
struct Inputs<'a> {
    provider: ProviderView<'a>,
    credential: CredentialView<'a>,
    operation: OperationKey,
    headers: &'a HeaderMap,
    path: &'a str,
    query: Option<&'a str>,
    body: Bytes,
    endpoint_override: Option<&'a str>,
}

impl AwsBedrock {
    /// Resolve, shape and sign one request. `now_secs` is a parameter so the
    /// clock is read once, by the caller, and signing stays replayable.
    fn signed(&self, inputs: Inputs<'_>, now_secs: u64) -> Result<request::Signed, ChannelError> {
        let config = BedrockConfig::from_view(inputs.provider)?;
        let accept = if inputs.operation
            == (OperationKey {
                operation: Operation::StreamGenerateContent,
                dialect: Dialect::OpenAiChat,
            }) {
            "text/event-stream"
        } else {
            "application/json"
        };
        let (plane, url, body) = match (inputs.operation.operation, inputs.operation.dialect) {
            (Operation::GenerateContent | Operation::StreamGenerateContent, Dialect::Claude) => {
                let model = endpoint::model_from_body(&inputs.body)?;
                let streaming = inputs.operation.operation == Operation::StreamGenerateContent;
                let url = endpoint::url(
                    inputs.provider,
                    &config,
                    Plane::Runtime,
                    &endpoint::invoke_path(&model, streaming),
                    None,
                    Some(&model),
                    inputs.endpoint_override,
                )?;
                let body = cache::shape(
                    inputs.body,
                    cache::rules_for(Dialect::Claude, config.enable_claude_magic_cache, false),
                );
                let body = request::invoke_body(&body, &config, inputs.headers)?;
                (Plane::Runtime, url, body)
            }
            (
                Operation::GenerateContent | Operation::StreamGenerateContent,
                Dialect::OpenAiChat,
            ) => {
                let model = endpoint::model_from_body(&inputs.body)?;
                let url = endpoint::url(
                    inputs.provider,
                    &config,
                    Plane::Runtime,
                    endpoint::CHAT_COMPLETIONS_PATH,
                    None,
                    Some(&model),
                    inputs.endpoint_override,
                )?;
                let body = cache::shape(
                    inputs.body,
                    cache::rules_for(Dialect::OpenAiChat, false, config.enable_openai_magic_cache),
                );
                (Plane::Runtime, url, body)
            }
            (Operation::ListModels, Dialect::OpenAi) => {
                let url = endpoint::url(
                    inputs.provider,
                    &config,
                    Plane::Control,
                    &endpoint::foundation_models_path(None),
                    endpoint::model_filters(inputs.query).as_deref(),
                    None,
                    inputs.endpoint_override,
                )?;
                (Plane::Control, url, Bytes::new())
            }
            (Operation::GetModel, Dialect::OpenAi) => {
                let model = endpoint::model_from_path(inputs.path)?;
                let url = endpoint::url(
                    inputs.provider,
                    &config,
                    Plane::Control,
                    &endpoint::foundation_models_path(Some(&model)),
                    None,
                    Some(&model),
                    inputs.endpoint_override,
                )?;
                (Plane::Control, url, Bytes::new())
            }
            _ => return Err(ChannelError::UnsupportedOperation(inputs.operation)),
        };
        request::build(
            inputs.provider,
            inputs.credential,
            &config,
            request::Target {
                plane,
                url,
                client_headers: inputs.headers,
                body,
                accept,
            },
            now_secs,
        )
    }

    /// The signed request for an overridden operation, whose body the host
    /// has already buffered.
    fn build(
        &self,
        operation: OperationKey,
        context: &OperationContext<'_>,
    ) -> Result<http::Request<HttpBody>, ChannelError> {
        let HttpBody::Bytes(body) = &context.request.body else {
            return Err(ChannelError::InvalidConfig(STREAMED_BODY.into()));
        };
        self.signed(
            Inputs {
                provider: context.provider,
                credential: context.credential,
                operation,
                headers: &context.request.headers,
                path: &context.request.path,
                query: context.request.query.as_deref(),
                body: body.clone(),
                endpoint_override: context.endpoint_override,
            },
            now_secs()?,
        )?
        .into_request()
    }
}

impl BaseChannel for AwsBedrock {
    fn claude_fallback(&self) -> Option<crate::channel::ClaudeFallback> {
        Some(crate::channel::ClaudeFallback {
            credit: true,
            recommended_model: "claude-opus-4-8",
        })
    }

    fn id(&self) -> &'static str {
        ID
    }

    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: ID,
            display_name: "AWS Bedrock",
            // An access key pair or a Bedrock API key is pasted; AWS grants
            // neither through an OAuth flow this channel could drive.
            login_modes: vec![LoginMode::ApiKey],
            capabilities: ChannelCapabilities::default(),
            config_keys: [
                ConfigKey::optional(
                    "region",
                    ConfigKeyKind::String,
                    "AWS region for both Bedrock planes and for the SigV4 credential scope; defaults to us-east-1.",
                ),
                ConfigKey::optional(
                    "base_url",
                    ConfigKeyKind::String,
                    "Origin replacing bedrock-runtime.{region}.amazonaws.com, for a VPC endpoint or a gateway. Provider column, not config JSON.",
                ).with_placeholder("https://bedrock-runtime.{region}.amazonaws.com"),
                ConfigKey::optional(
                    "control_base_url",
                    ConfigKeyKind::String,
                    "Origin replacing bedrock.{region}.amazonaws.com, which serves the foundation-model directory.",
                ).with_placeholder("https://bedrock.{region}.amazonaws.com"),
                ConfigKey::optional(
                    "anthropic_version",
                    ConfigKeyKind::String,
                    "The anthropic_version written into every Anthropic request body; defaults to bedrock-2023-05-31.",
                ),
                ConfigKey::optional(
                    "enable_claude_magic_cache",
                    ConfigKeyKind::Bool,
                    "Turn a client's magic cache string in an Anthropic model's body into cache_control. The strings are stripped either way.",
                ),
                ConfigKey::optional(
                    "enable_openai_magic_cache",
                    ConfigKeyKind::Bool,
                    "Turn a client's magic cache string in a Chat Completions body (non-Anthropic models) into prompt_cache_breakpoint.",
                ),
            ]
            .into_iter()
            .chain(crate::channel::CLAUDE_FALLBACK_KEYS)
            .chain(HOST_CONFIG_KEYS)
            .collect(),
        }
    }

    /// AWS authenticates by signature, not by client identity: there is no
    /// vendor CLI fingerprint worth impersonating, so the host's own default
    /// client serves.
    fn default_connection(&self) -> Option<gproxy_client::ConnectionConfig> {
        None
    }

    fn native_dialects(&self, _provider: ProviderView<'_>, operation: Operation) -> Vec<Dialect> {
        match operation {
            Operation::GenerateContent | Operation::StreamGenerateContent => vec![Dialect::Claude],
            Operation::ListModels | Operation::GetModel => vec![Dialect::OpenAi],
            _ => Vec::new(),
        }
    }

    /// Generation takes the wire its model is served on: Messages for
    /// Anthropic models, Chat Completions for the rest (`serves_messages`).
    fn native_dialects_for_model(
        &self,
        provider: ProviderView<'_>,
        operation: Operation,
        model: Option<&str>,
    ) -> Vec<Dialect> {
        match (operation, model) {
            (Operation::GenerateContent | Operation::StreamGenerateContent, Some(model))
                if !endpoint::serves_messages(model) =>
            {
                vec![Dialect::OpenAiChat]
            }
            _ => self.native_dialects(provider, operation),
        }
    }

    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        // SigV4 hashes the payload, so the body has to be complete. The host
        // buffers generation bodies; a streamed one cannot be signed.
        let HttpBody::Bytes(body) = ctx.request.body else {
            return Err(ChannelError::InvalidConfig(STREAMED_BODY.into()));
        };
        self.signed(
            Inputs {
                provider: ctx.provider,
                credential: ctx.credential,
                operation: ctx.operation,
                headers: &ctx.request.headers,
                path: &ctx.request.path,
                query: ctx.request.query.as_deref(),
                body,
                endpoint_override: ctx.endpoint_override,
            },
            now_secs()?,
        )?
        .into_request()
    }

    /// The directory is not an OpenAI list until the channel makes it one.
    fn list_models<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(directory(self, Operation::ListModels, context))
    }

    fn get_model<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(directory(self, Operation::GetModel, context))
    }

    /// An InvokeModel reply is AWS event-stream framing; the client asked for
    /// Messages SSE, so the body is translated frame by frame as it arrives.
    /// Chat Completions already answers in SSE and passes through.
    fn stream_generate_content<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(async move {
            let operation = OperationKey {
                operation: Operation::StreamGenerateContent,
                dialect: context.dialect,
            };
            let request = self.build(operation, &context)?;
            let response = context.client.send(request).await?;
            if !response.status.is_success() || context.dialect == Dialect::OpenAiChat {
                return Ok(response);
            }
            let mut headers = response.headers;
            headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/event-stream"),
            );
            headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
            headers.remove(header::CONTENT_LENGTH);
            Ok(WireResponse {
                status: response.status,
                headers,
                body: HttpBody::Stream(translate(response.body)),
            })
        })
    }

    fn response_reason_observer(
        &self,
        _status: http::StatusCode,
        headers: &http::HeaderMap,
        max_bytes: u64,
    ) -> Option<Box<dyn crate::channel::ResponseReasonObserver>> {
        crate::channels::shared::aws_reason::observer(headers, max_bytes, true)
    }
}

/// `ListModels`/`GetModel`: send, then rewrite a successful AWS reply into the
/// OpenAI shape the channel declared. A failure is returned as it arrived.
async fn directory(
    channel: &AwsBedrock,
    operation: Operation,
    context: OperationContext<'_>,
) -> Result<WireResponse<HttpBody>, ChannelError> {
    let key = OperationKey {
        operation,
        dialect: context.dialect,
    };
    let request = channel.build(key, &context)?;
    let response = context.client.send(request).await?;
    if !response.status.is_success() {
        return Ok(response);
    }
    let body = read_body(response.body, MAX_MODEL_BODY).await?;
    let rewritten = models::rewrite(&body, operation == Operation::GetModel)?;
    let mut headers = response.headers;
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.remove(header::CONTENT_LENGTH);
    Ok(WireResponse {
        status: StatusCode::OK,
        headers,
        body: HttpBody::Bytes(Bytes::from(rewritten)),
    })
}

enum Translating {
    Reading(ByteStream, stream::Translator),
    Done,
}

/// Wrap an event-stream body in the Messages SSE translation. A decoding
/// failure ends the stream as a transport error, so the host records an
/// interrupted response rather than a complete one.
fn translate(body: HttpBody) -> ByteStream {
    let upstream: ByteStream = match body {
        HttpBody::Stream(stream) => stream,
        HttpBody::Bytes(bytes) => Box::pin(futures_util::stream::once(async move { Ok(bytes) })),
    };
    Box::pin(futures_util::stream::unfold(
        Translating::Reading(upstream, stream::Translator::new()),
        |state| async move {
            let Translating::Reading(mut upstream, mut translator) = state else {
                return None;
            };
            loop {
                match upstream.next().await {
                    Some(Ok(chunk)) => {
                        let out = match translator.push(&chunk) {
                            Ok(out) => out,
                            Err(error) => {
                                return Some((
                                    Err(stream::transport_error(error)),
                                    Translating::Done,
                                ));
                            }
                        };
                        if translator.failed() {
                            return Some((Ok(out), Translating::Done));
                        }
                        if !out.is_empty() {
                            return Some((Ok(out), Translating::Reading(upstream, translator)));
                        }
                    }
                    Some(Err(error)) => return Some((Err(error), Translating::Done)),
                    None => {
                        return match translator.finish() {
                            Ok(()) => None,
                            Err(error) => {
                                Some((Err(stream::transport_error(error)), Translating::Done))
                            }
                        };
                    }
                }
            }
        },
    ))
}

async fn read_body(body: HttpBody, limit: usize) -> Result<Bytes, ChannelError> {
    match body {
        HttpBody::Bytes(bytes) => Ok(bytes),
        HttpBody::Stream(mut stream) => {
            let mut out = Vec::new();
            while let Some(chunk) = stream.next().await {
                let chunk =
                    chunk.map_err(|error| ChannelError::InvalidResponse(error.to_string()))?;
                out.extend_from_slice(&chunk);
                if out.len() > limit {
                    return Err(ChannelError::InvalidResponse(
                        "the AWS model directory exceeds the read limit".into(),
                    ));
                }
            }
            Ok(Bytes::from(out))
        }
    }
}

/// Unix seconds. `web-time` keeps this working on wasm, where
/// `std::time::SystemTime::now` is unavailable.
fn now_secs() -> Result<u64, ChannelError> {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .map_err(|_| ChannelError::InvalidConfig("the system clock is before 1970".into()))
}

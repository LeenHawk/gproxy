//! One generation turn against claude.ai, driven entirely by the channel
//! (v3 `claudeweb/orchestrator/`). A new turn uploads attachments, creates
//! a temporary conversation, sets its settings and posts the completion; the
//! returned body is the translated Messages stream, and the conversation is
//! deleted when that stream ends. A turn that stops at a `tool_use` keeps
//! the completion connection open (claude.ai continues on it once the tool
//! result is posted), records the session under `tool:{tool_use_id}` in the
//! host's state, and parks the live connection in the channel's registry.
//! The tool-result turn reads that key, posts each result to
//! `/tool_result`, resumes the parked stream, and cleans up when the answer
//! holds no further tool call.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use futures_util::StreamExt;
use gproxy_protocol::{
    HttpBody, WireResponse,
    capability::{CasResult, StateWrite, Version},
    connection::{ByteStream, Bytes, TransportError},
};
use http::StatusCode;
use serde_json::Value;

use super::{
    ClaudeWeb, ClaudeWebConfig, MAX_REQUEST_BODY, MAX_SERVICE_BODY, auth, bad_request, expired, id,
    now_ms,
    prepare::Requests,
    read_body, request,
    stream::{Codec, Collector, SessionState, encode_all},
    system_time,
};
use crate::{
    OutboundClient,
    channel::{ChannelError, ChannelState, HeaderAllowlist, OperationContext, forwardable},
};

/// How long a tool boundary stays resumable, as in v3 (`ttl_secs`).
pub const CONTINUATION_SECS: u64 = 10 * 60;

/// A completion connection waiting for its tool result.
struct Parked {
    upstream: ByteStream,
    codec: Codec,
    parked_at_ms: i64,
}

/// The registry needs `Send` to satisfy `BaseChannel: Send + Sync`. On
/// wasm32 the JS-backed body stream is not `Send`; `SendWrapper` asserts the
/// single-threaded runtime and panics rather than crossing a thread.
#[cfg(not(target_arch = "wasm32"))]
type Slot = Parked;
#[cfg(target_arch = "wasm32")]
type Slot = send_wrapper::SendWrapper<Parked>;

#[cfg(not(target_arch = "wasm32"))]
fn slot(parked: Parked) -> Slot {
    parked
}
#[cfg(target_arch = "wasm32")]
fn slot(parked: Parked) -> Slot {
    send_wrapper::SendWrapper::new(parked)
}
#[cfg(not(target_arch = "wasm32"))]
fn unslot(slot: Slot) -> Parked {
    slot
}
#[cfg(target_arch = "wasm32")]
fn unslot(slot: Slot) -> Parked {
    slot.take()
}

/// Live connections cannot be serialized into `ChannelState`, so they wait
/// here, keyed by provider, credential and tool_use id. Entries older than
/// the continuation window are dropped on the next access; the temporary
/// conversation behind them is discarded by claude.ai itself.
#[derive(Default)]
pub struct Registry {
    parked: Mutex<HashMap<String, Slot>>,
}

impl Registry {
    fn park(&self, key: String, parked: Parked) {
        let now = parked.parked_at_ms;
        let mut map = self.parked.lock().unwrap_or_else(|e| e.into_inner());
        purge(&mut map, now);
        map.insert(key, slot(parked));
    }

    fn take(&self, key: &str, now: i64) -> Option<Parked> {
        let mut map = self.parked.lock().unwrap_or_else(|e| e.into_inner());
        purge(&mut map, now);
        map.remove(key).map(unslot)
    }

    /// Continuations this instance currently holds; visible for tests.
    pub fn len(&self) -> usize {
        self.parked.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

fn purge(map: &mut HashMap<String, Slot>, now: i64) {
    let ttl = i64::try_from(CONTINUATION_SECS * 1000).unwrap_or(i64::MAX);
    map.retain(|_, parked| now.saturating_sub(parked.parked_at_ms) < ttl);
}

pub fn continuation_key(tool_use_id: &str) -> String {
    format!("tool:{tool_use_id}")
}

pub(super) struct Turn {
    client: Arc<dyn OutboundClient>,
    state: Arc<dyn ChannelState>,
    registry: Arc<Registry>,
    scope: String,
    requests: Requests,
    conversation: String,
    live: Option<(ByteStream, Codec)>,
    /// The continuation entry a tool-result turn is consuming.
    claimed: Option<(String, Version)>,
    parked: bool,
}

pub(super) async fn start(
    channel: &ClaudeWeb,
    context: OperationContext<'_>,
) -> Result<Turn, ChannelError> {
    let config = ClaudeWebConfig::from_view(context.provider)?;
    let auth = auth::Auth::read(&context.credential)?;
    let base = auth::base_url(context.provider);
    let allowlist = HeaderAllowlist::from_view(context.provider)?;
    let mut headers = forwardable(
        &context.request.headers,
        allowlist.as_ref(),
        auth::CHANNEL_HEADERS,
    );
    config.static_headers(&mut headers)?;
    let body = read_body(context.request.body, MAX_REQUEST_BODY).await?;
    let value = request::parse(&body)?;
    let results = request::tool_results(&value);
    let requests = Requests::new(auth, base, &config, headers, context.endpoint_override);
    let scope = format!("{}/{}", context.provider.id, context.credential.id);
    let client = context.client;
    let state = context.state;
    let instance = context.instance_id.to_string();
    let registry = channel.registry.clone();
    if results.is_empty() {
        new_turn(
            client, state, registry, scope, requests, instance, &config, &value,
        )
        .await
    } else {
        resume_turn(client, state, registry, scope, requests, instance, results).await
    }
}

#[allow(clippy::too_many_arguments)]
async fn new_turn(
    client: Arc<dyn OutboundClient>,
    state: Arc<dyn ChannelState>,
    registry: Arc<Registry>,
    scope: String,
    requests: Requests,
    instance: String,
    config: &ClaudeWebConfig,
    value: &Value,
) -> Result<Turn, ChannelError> {
    let mut web = request::build(value, &config.prompt, &config.timezone)?;
    let conversation = id::uuid()?;
    let mut files = Vec::new();
    for upload in &web.uploads {
        let (status, bytes) = call(&*client, requests.upload(&conversation, upload)?).await?;
        if !status.is_success() {
            return Err(ChannelError::UpstreamResponse {
                status,
                body: bytes,
            });
        }
        let reply: Value = serde_json::from_slice(&bytes)
            .map_err(|error| ChannelError::InvalidResponse(format!("upload JSON: {error}")))?;
        let file = reply
            .get("file_uuid")
            .and_then(Value::as_str)
            .ok_or_else(|| ChannelError::InvalidResponse("upload file_uuid missing".into()))?;
        files.push(Value::String(file.to_owned()));
    }
    let (status, bytes) = call(&*client, requests.create(&conversation)?).await?;
    if !status.is_success() {
        return Err(ChannelError::UpstreamResponse {
            status,
            body: bytes,
        });
    }
    let settings = requests.settings(&conversation, web.extended)?;
    let (status, bytes) = match call(&*client, settings).await {
        Ok(reply) => reply,
        Err(error) => {
            discard(&*client, &requests, &conversation).await;
            return Err(error);
        }
    };
    if !status.is_success() {
        discard(&*client, &requests, &conversation).await;
        return Err(ChannelError::UpstreamResponse {
            status,
            body: bytes,
        });
    }
    web.body["files"] = Value::Array(files);
    let completion = requests.completion(&conversation, &web.body)?;
    let response = match client.send(completion).await {
        Ok(response) => response,
        Err(error) => {
            discard(&*client, &requests, &conversation).await;
            return Err(error.into());
        }
    };
    if !response.status.is_success() {
        let body = read_body(response.body, MAX_SERVICE_BODY)
            .await
            .unwrap_or_default();
        discard(&*client, &requests, &conversation).await;
        return Err(ChannelError::UpstreamResponse {
            status: response.status,
            body,
        });
    }
    let session = SessionState {
        conversation: conversation.clone(),
        model: web.model,
        message_id: id::fresh("msg")?,
        input_tokens: web.input_tokens,
        instance,
    };
    Ok(Turn {
        client,
        state,
        registry,
        scope,
        requests,
        conversation,
        live: Some((body_stream(response.body), Codec::new(session))),
        claimed: None,
        parked: false,
    })
}

async fn resume_turn(
    client: Arc<dyn OutboundClient>,
    state: Arc<dyn ChannelState>,
    registry: Arc<Registry>,
    scope: String,
    requests: Requests,
    instance: String,
    results: Vec<Value>,
) -> Result<Turn, ChannelError> {
    let tool_use_id = results
        .first()
        .and_then(|result| result.get("tool_use_id"))
        .and_then(Value::as_str)
        .ok_or_else(|| bad_request("tool_result id missing"))?
        .to_owned();
    if results.iter().any(|result| {
        result.get("tool_use_id").and_then(Value::as_str) != Some(tool_use_id.as_str())
    }) {
        return Err(bad_request("tool results refer to different continuations"));
    }
    let key = continuation_key(&tool_use_id);
    let entry = state
        .get(&key)
        .await?
        .ok_or_else(|| expired("continuation expired or unknown"))?;
    let mut session: SessionState = serde_json::from_slice(&entry.payload)
        .map_err(|error| ChannelError::InvalidResponse(format!("continuation state: {error}")))?;
    session.input_tokens = session.input_tokens.saturating_add(
        results
            .iter()
            .map(|value| request::estimate_tokens(&value.to_string()))
            .sum::<u64>(),
    );
    let conversation = session.conversation.clone();
    if session.instance != instance {
        // The parked connection lives in another process; it and the
        // continuation record stay intact for the request to be routed there.
        return Err(ChannelError::ContinuationElsewhere {
            instance_id: session.instance,
        });
    }
    let now = now_ms()?;
    let Some(parked) = registry.take(&format!("{scope}/{tool_use_id}"), now) else {
        // Another instance (or an expired window) holds the connection; the
        // continuation cannot be served, so release it entirely.
        let _ = state
            .compare_exchange(&key, Some(entry.version), None)
            .await;
        discard(&*client, &requests, &conversation).await;
        return Err(expired("continuation is not held by this instance"));
    };
    for result in &results {
        let outcome = match requests.tool_result(&conversation, result) {
            Ok(request) => call(&*client, request).await,
            Err(error) => Err(error),
        };
        let failure = match outcome {
            Ok((status, _)) if status.is_success() => continue,
            Ok((status, body)) => ChannelError::UpstreamResponse { status, body },
            Err(error) => error,
        };
        let _ = state
            .compare_exchange(&key, Some(entry.version), None)
            .await;
        discard(&*client, &requests, &conversation).await;
        return Err(failure);
    }
    let Parked {
        upstream,
        mut codec,
        ..
    } = parked;
    codec.resume(session);
    Ok(Turn {
        client,
        state,
        registry,
        scope,
        requests,
        conversation,
        live: Some((upstream, codec)),
        claimed: Some((key, entry.version)),
        parked: false,
    })
}

impl Turn {
    /// The next batch of translated events; `None` once the message ended
    /// (after cleanup) or the turn was parked at a tool boundary.
    pub(super) async fn step(&mut self) -> Option<Result<Vec<Value>, TransportError>> {
        loop {
            let next = match self.live.as_mut() {
                Some((upstream, _)) => upstream.next().await,
                None => return None,
            };
            match next {
                Some(Ok(chunk)) => {
                    let pushed = match self.live.as_mut() {
                        Some((_, codec)) => codec.push(&chunk),
                        None => return None,
                    };
                    let output = match pushed {
                        Ok(output) => output,
                        Err(error) => {
                            self.live = None;
                            self.cleanup().await;
                            return Some(Err(Box::new(error)));
                        }
                    };
                    if let Some(tool_use_id) = output.tool_use
                        && let Err(error) = self.park(&tool_use_id).await
                    {
                        self.cleanup().await;
                        return Some(Err(Box::new(error)));
                    }
                    if output.events.is_empty() {
                        continue;
                    }
                    return Some(Ok(output.events));
                }
                Some(Err(error)) => {
                    self.live = None;
                    self.cleanup().await;
                    return Some(Err(error));
                }
                None => {
                    let finished = self.live.take().map(|(_, mut codec)| codec.finish());
                    self.cleanup().await;
                    return match finished {
                        Some(Ok(events)) if events.is_empty() => None,
                        Some(Ok(events)) => Some(Ok(events)),
                        Some(Err(error)) => Some(Err(Box::new(error))),
                        None => None,
                    };
                }
            }
        }
    }

    /// The Messages SSE body. Cleanup runs when the upstream stream ends; a
    /// body dropped before that leaves only a temporary conversation behind.
    pub(super) fn into_stream(self) -> ByteStream {
        Box::pin(futures_util::stream::unfold(self, |mut turn| async move {
            let item = turn.step().await?;
            Some((item.map(|events| encode_all(&events)), turn))
        }))
    }

    /// Drain the turn into one Messages response body.
    pub(super) async fn collect(mut self) -> Result<Value, ChannelError> {
        let mut collector = Collector::default();
        while let Some(item) = self.step().await {
            let events = match item {
                Ok(events) => events,
                Err(error) => return Err(ChannelError::InvalidResponse(error.to_string())),
            };
            for event in &events {
                if let Err(error) = collector.push(event) {
                    self.live = None;
                    self.cleanup().await;
                    return Err(error);
                }
            }
        }
        Ok(collector.finish())
    }

    /// Record the continuation and hand the live connection to the registry.
    async fn park(&mut self, tool_use_id: &str) -> Result<(), ChannelError> {
        let Some((upstream, codec)) = self.live.take() else {
            return Ok(());
        };
        let payload = serde_json::to_vec(codec.state())
            .map_err(|error| ChannelError::InvalidResponse(error.to_string()))?;
        let now = now_ms()?;
        let expires_at = now.saturating_add(i64::try_from(CONTINUATION_SECS * 1000).unwrap_or(0));
        let write = StateWrite {
            payload: Bytes::from(payload),
            expires_at: Some(system_time(expires_at)),
        };
        let key = continuation_key(tool_use_id);
        let result = self.state.compare_exchange(&key, None, Some(write)).await?;
        if !matches!(result, CasResult::Applied(_)) {
            return Err(ChannelError::InvalidResponse(format!(
                "continuation `{tool_use_id}` is already recorded"
            )));
        }
        if let Some((previous, version)) = self.claimed.take() {
            let _ = self
                .state
                .compare_exchange(&previous, Some(version), None)
                .await;
        }
        self.parked = true;
        self.registry.park(
            format!("{}/{tool_use_id}", self.scope),
            Parked {
                upstream,
                codec,
                parked_at_ms: now,
            },
        );
        Ok(())
    }

    /// Release the continuation entry and delete the conversation. Does
    /// nothing for a parked turn: the conversation is still in use.
    async fn cleanup(&mut self) {
        if self.parked {
            return;
        }
        if let Some((key, version)) = self.claimed.take() {
            let _ = self.state.compare_exchange(&key, Some(version), None).await;
        }
        discard(&*self.client, &self.requests, &self.conversation).await;
    }
}

/// `DELETE` the conversation; failures are not reported anywhere.
async fn discard(client: &dyn OutboundClient, requests: &Requests, conversation: &str) {
    if let Ok(request) = requests.cleanup(conversation) {
        let _ = client.send(request).await;
    }
}

async fn call(
    client: &dyn OutboundClient,
    request: http::Request<HttpBody>,
) -> Result<(StatusCode, Bytes), ChannelError> {
    let WireResponse { status, body, .. } = client.send(request).await?;
    Ok((status, read_body(body, MAX_SERVICE_BODY).await?))
}

fn body_stream(body: HttpBody) -> ByteStream {
    match body {
        HttpBody::Stream(stream) => stream,
        HttpBody::Bytes(bytes) => Box::pin(futures_util::stream::once(async move { Ok(bytes) })),
    }
}

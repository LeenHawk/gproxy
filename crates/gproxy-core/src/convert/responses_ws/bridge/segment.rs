//! One HTTP generation segment. The session owns cancellation and logical
//! response IDs; this driver only reuses the existing concrete stream adapters.

use super::*;
use futures_util::StreamExt;
use gproxy_protocol::{
    HttpBody,
    adapt::generate::{
        GenerationIdentity,
        stream::{StreamInvocation, StreamStart, StreamTarget, bridge::StreamBridge},
    },
    capability::Upstream,
    codec::{SseDecoder, SseFrame},
};
use tokio::sync::oneshot;

pub(super) struct Segment {
    pub events: mpsc::Receiver<Result<Value, (u16, String)>>,
    cancellation: CancellationToken,
    done: Option<oneshot::Receiver<()>>,
}

impl Segment {
    pub async fn stop(mut self) {
        self.cancellation.cancel();
        if let Some(done) = self.done.take() {
            let _ = done.await;
        }
    }
}
impl Drop for Segment {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}

pub(super) fn start<C: BatchConnectionTrait + Send + Sync + 'static>(
    config: Arc<Config<C>>,
    turn: Arc<crate::responses::Turn>,
    mut input: r::GenerateContentRequestBody,
) -> Segment {
    let (tx, events) = mpsc::channel(super::super::FRAME_QUEUE);
    let (done, receiver) = oneshot::channel();
    let cancellation = CancellationToken::new();
    let cancel = cancellation.clone();
    input.model = Some(config.model.clone());
    // The logical response owns its history; a segment must not publish a
    // second previous_response_id or save the transcript under a hidden ID.
    input.store = Some(Some(false));
    input.stream = Some(Some(true));
    input.previous_response_id = None;
    if config.target == Dialect::Claude && input.max_output_tokens.flatten().is_none() {
        input.max_output_tokens = Some(Some(
            turn.attempt
                .request
                .target
                .provider
                .models
                .iter()
                .find(|m| m.upstream_name == config.model)
                .and_then(|m| m.metadata.get("max_output_tokens").and_then(Value::as_i64))
                .unwrap_or(8192),
        ));
    }
    crate::rt::spawn(async move {
        let drive = async {
            let upstream = AttemptUpstream::new(
                turn.funnel.clone(),
                turn.attempt.clone(),
                config.headers.clone(),
                turn.attempt.request.snapshot.limits.capability(None),
                config.channel_state.clone(),
                config.instance_id.clone(),
            );
            let result = drive(&config, &upstream, input, &tx).await;
            if let Err(error) = result {
                let _ = tx.send(Err(error)).await;
            }
        };
        tokio::select! {
            biased;
            () = cancel.cancelled() => {},
            () = turn.attempt.request.cancellation.cancelled() => {},
            () = drive => {},
        }
        drop(tx);
        let _ = done.send(());
    });
    Segment {
        events,
        cancellation,
        done: Some(receiver),
    }
}

async fn drive<C: BatchConnectionTrait + Send + Sync + 'static>(
    config: &Config<C>,
    upstream: &AttemptUpstream,
    input: r::GenerateContentRequestBody,
    tx: &mpsc::Sender<Result<Value, (u16, String)>>,
) -> Result<(), (u16, String)> {
    let key = OperationKey {
        operation: Operation::StreamGenerateContent,
        dialect: config.target,
    };
    if config.target == Dialect::OpenAi {
        let bytes =
            gproxy_protocol::codec::encode_json(&input, config.settings.codec).map_err(bad)?;
        let response = upstream
            .send(
                &key,
                WireRequest {
                    method: http::Method::POST,
                    path: config.endpoint.path.clone(),
                    query: config.endpoint.query.clone(),
                    headers: HeaderMap::new(),
                    body: HttpBody::Bytes(bytes),
                },
            )
            .await
            .map_err(bad)?;
        if !response.status.is_success() {
            let status = response.status.as_u16();
            let bytes = collect(response.body).await;
            return Err((status, upstream_message(&bytes, status)));
        }
        let mut stream = stream(response.body);
        let mut decoder = SseDecoder::new(config.settings.codec);
        while let Some(chunk) = stream.next().await {
            for frame in decoder.push(&chunk.map_err(bad)?).map_err(bad)? {
                if let SseFrame::Event(event) = frame {
                    let event = serde_json::from_str(&event.data).map_err(bad)?;
                    tx.send(Ok(event)).await.map_err(bad)?;
                }
            }
        }
        decoder.finish().map_err(bad)?;
        return Ok(());
    }
    let state = config.owned.access();
    let target = StreamTarget {
        endpoint: config.endpoint.clone(),
        identities: GenerationIdentity::new(
            super::super::namespace(),
            super::super::namespace(),
            Dialect::OpenAi,
            config.target,
        )
        .map_err(bad)?,
    };
    let created = now_ms().div_euclid(1000);
    macro_rules! run {
        ($pair:ty, $facts:expr) => {{
            let facts = $facts;
            let invocation = <$pair>::prepare_stream(input, target, facts, config.settings, &state)
                .await
                .map_err(bad)?;
            forward(invocation, upstream, &key, &state, tx).await
        }};
    }
    match config.target {
        Dialect::OpenAiChat => run!(
            super::super::ResponsesViaChat,
            super::super::responses_over_chat_context(&input)
        ),
        Dialect::Claude => run!(
            super::super::ResponsesViaClaude,
            super::super::responses_over_claude_facts(&input, created)
        ),
        Dialect::Gemini => run!(
            super::super::ResponsesViaGemini,
            super::super::responses_over_gemini_facts(&input, created, &config.model)
        ),
        _ => Err((400, "unsupported Responses bridge target".into())),
    }
}

async fn forward<B, C>(
    mut invocation: StreamInvocation<B>,
    upstream: &AttemptUpstream,
    key: &OperationKey,
    state: &gproxy_protocol::adapt::generate::GenerationStateAccess<'_, ProtocolState<C>>,
    tx: &mpsc::Sender<Result<Value, (u16, String)>>,
) -> Result<(), (u16, String)>
where
    B: StreamBridge<ClientEvent = r::stream::StreamEvent>,
    C: BatchConnectionTrait + Send + Sync,
{
    match invocation.start(upstream, key, state).await.map_err(bad)? {
        StreamStart::Rejected(response) => Err((
            response.status.as_u16(),
            upstream_message(&response.body, response.status.as_u16()),
        )),
        StreamStart::Streaming(_) => {
            while let Some(chunk) = invocation.next(state).await.map_err(bad)? {
                if let Some(event) = chunk.event {
                    tx.send(Ok(serde_json::to_value(event).map_err(bad)?))
                        .await
                        .map_err(bad)?;
                }
            }
            Ok(())
        }
    }
}

fn bad(error: impl std::fmt::Display) -> (u16, String) {
    (502, error.to_string())
}

async fn collect(body: gproxy_protocol::HttpBody) -> Vec<u8> {
    let mut stream = stream(body);
    let mut result = Vec::new();
    while let Some(Ok(bytes)) = stream.next().await {
        result.extend_from_slice(&bytes);
    }
    result
}

fn stream(body: HttpBody) -> gproxy_protocol::connection::ByteStream {
    match body {
        HttpBody::Bytes(bytes) => Box::pin(futures_util::stream::once(async move { Ok(bytes) })),
        HttpBody::Stream(stream) => stream,
    }
}

fn upstream_message(bytes: &[u8], status: u16) -> String {
    serde_json::from_slice::<Value>(bytes)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().map(str::to_owned))
        .unwrap_or_else(|| format!("upstream rejected generation ({status})"))
}

//! One HTTP generation segment, executed through the admitted retry plan.

use super::*;
use futures_util::StreamExt;
use gproxy_protocol::{
    HttpBody,
    codec::{SseDecoder, SseFrame},
};
use tokio::sync::oneshot;

mod failure;
pub(super) use failure::Failure;

pub(super) struct Segment {
    pub events: mpsc::Receiver<Result<Value, Failure>>,
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

pub(super) fn start(
    settings: StreamSettings,
    headers: HeaderMap,
    session: crate::ResponsesSession,
    turn: Arc<crate::responses::Turn>,
    mut input: r::GenerateContentRequestBody,
) -> Segment {
    let (tx, events) = mpsc::channel(super::super::FRAME_QUEUE);
    let (done, receiver) = oneshot::channel();
    let cancellation = CancellationToken::new();
    let cancel = cancellation.clone();
    // The logical response owns its history. Segments never publish hidden
    // response IDs or ask an upstream to resolve the gateway's previous ID.
    input.store = Some(Some(false));
    input.stream = Some(Some(true));
    input.previous_response_id = None;
    crate::rt::spawn(async move {
        let drive = async {
            let result = async {
                let executor = turn
                    .http_executor
                    .as_ref()
                    .ok_or_else(|| Failure::new(500, "Responses HTTP executor is required"))?;
                let bytes = gproxy_protocol::codec::encode_json(&input, settings.codec)
                    .map_err(Failure::transport)?;
                let response = executor
                    .send(
                        &session,
                        WireRequest {
                            method: http::Method::POST,
                            path: "/v1/responses".into(),
                            query: None,
                            headers,
                            body: HttpBody::Bytes(bytes),
                        },
                    )
                    .await
                    .map_err(Failure::core)?;
                if !response.status.is_success() {
                    let status = response.status.as_u16();
                    let mut body = stream(response.body);
                    let mut bytes = Vec::new();
                    while let Some(chunk) = body.next().await {
                        bytes.extend_from_slice(&chunk.map_err(Failure::transport)?);
                    }
                    return Err(Failure::rejected(status, &bytes));
                }
                let mut stream = stream(response.body);
                let mut decoder = SseDecoder::new(settings.codec);
                while let Some(chunk) = stream.next().await {
                    for frame in decoder
                        .push(&chunk.map_err(Failure::transport)?)
                        .map_err(Failure::transport)?
                    {
                        if let SseFrame::Event(event) = frame {
                            if event.data.trim() == "[DONE]" {
                                continue;
                            }
                            let event =
                                serde_json::from_str(&event.data).map_err(Failure::transport)?;
                            tx.send(Ok(event)).await.map_err(Failure::transport)?;
                        }
                    }
                }
                decoder.finish().map_err(Failure::transport)?;
                Ok(())
            }
            .await;
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

fn stream(body: HttpBody) -> gproxy_protocol::connection::ByteStream {
    match body {
        HttpBody::Bytes(bytes) => Box::pin(futures_util::stream::once(async move { Ok(bytes) })),
        HttpBody::Stream(stream) => stream,
    }
}

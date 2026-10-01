//! Responses HTTP/SSE over a native Responses socket, without a foreign
//! protocol round trip. The native helper validates one complete response.

use super::*;
use futures_util::StreamExt;
use gproxy_protocol::wire::openai::responses::stream::StreamEvent;
use gproxy_protocol::{
    HttpBody,
    adapt::responses_ws::{self, ResponsesWsConnect},
    codec::{SseEvent, encode_sse_event},
    wire::openai::responses as r,
};

pub(super) async fn over_websocket<C: BatchConnectionTrait + Send + Sync + 'static>(
    call: &Call<'_, C>,
    settings: StreamSettings,
) -> Result<Converted, TransformError> {
    let mut request: r::GenerateContentRequestBody = decode(call.body(), call.limits)?;
    request.model = call.model.map(str::to_owned);
    let mut session = match responses_ws::connect(
        call.upstream,
        &WS_KEY,
        handshake_request(),
        ResponsesWsLimits::default(),
    )
    .await
    {
        Ok(ResponsesWsConnect::Connected { session, .. }) => session,
        Ok(ResponsesWsConnect::Rejected(response)) => return Ok(Converted::Rejected(response)),
        Err(error) => return Err(error.error),
    };
    if call.collect {
        let mut turn = session.turn(request)?;
        let mut response = None;
        while let Some(event) = turn.next().await {
            match event? {
                StreamEvent::Completed(event) => response = Some(event.response),
                StreamEvent::Incomplete(event) => response = Some(event.response),
                StreamEvent::Failed(event) => response = Some(event.response),
                _ => {}
            }
        }
        let response = response.ok_or_else(|| {
            TransformError::invalid_result("responses.websocket", "missing terminal response")
        })?;
        return Ok(Converted::Success(WireResponse {
            status: StatusCode::OK,
            headers: HeaderMap::from_iter([(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_static("application/json"),
            )]),
            body: HttpBody::Bytes(
                gproxy_protocol::codec::encode_json(&response, settings.codec).map_err(|e| {
                    TransformError::invalid_result("responses.websocket", e.to_string())
                })?,
            ),
        }));
    }
    let (tx, rx) = mpsc::channel(FRAME_QUEUE);
    crate::rt::spawn(async move {
        let drive = async {
            let mut turn = session.turn(request)?;
            while let Some(event) = turn.next().await {
                let event = serde_json::to_value(event?)?;
                let event = SseEvent {
                    event: event["type"].as_str().map(str::to_owned),
                    id: None,
                    data: event.to_string(),
                    retry: None,
                };
                let bytes = encode_sse_event(&event, settings.codec).map_err(|e| {
                    TransformError::invalid_result("responses.websocket", e.to_string())
                })?;
                if tx.send(Ok(bytes)).await.is_err() {
                    return Ok::<_, TransformError>(());
                }
            }
            Ok(())
        };
        if let Err(error) = drive.await {
            let _ = tx.send(Err(transport(error))).await;
        }
    });
    let body = Box::pin(futures_util::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|chunk| (chunk, rx))
    }));
    Ok(Converted::Stream(WireResponse {
        status: StatusCode::OK,
        headers: stream_headers(settings.client_framing),
        body,
    }))
}

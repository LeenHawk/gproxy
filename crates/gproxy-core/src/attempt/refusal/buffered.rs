use bytes::Bytes;
use gproxy_channel_api::{NormalizedUsage, ResponseShapeCtx, UsageCtx};
use serde_json::{Value, json};

use crate::boundary::ByteStream;
use crate::error::CoreError;
use crate::host::Host;

use super::retry::{Runner, as_stream};

pub(super) async fn run<H: Host>(
    runner: &mut Runner<H>,
    mut response: http::Response<ByteStream>,
) -> http::Response<ByteStream> {
    let mut previous = None;
    loop {
        let collected = match crate::attempt::body::collect(response).await {
            Ok(response) => response,
            Err(error) => {
                runner.meter.record(
                    NormalizedUsage {
                        input_tokens: crate::usage::estimate_input_tokens(&runner.replay.body),
                        output_tokens: crate::usage::utf8_chars(&error.body).div_ceil(2),
                        ..Default::default()
                    },
                    runner.facts.target.upstream_model.clone(),
                    runner.facts.upstream_started_at_ms.expect("send time"),
                    false,
                    true,
                );
                runner
                    .capture(error.status, &error.headers, error.body)
                    .await;
                return failure(CoreError::Transport(error.error));
            }
        };
        runner
            .capture(
                collected.status(),
                collected.headers(),
                collected.body().clone(),
            )
            .await;
        if !collected.status().is_success() {
            if matches!(collected.status().as_u16(), 429 | 503)
                && let Some((parts, mut body)) = previous
            {
                recommended(&mut body, &runner.facts.target.upstream_model);
                return finish(runner, parts, body).await;
            }
            return as_stream(collected);
        }
        let channel = runner
            .core
            .channels
            .get(&runner.facts.target.provider.channel)
            .expect("prepared channel");
        let key = runner.facts.key.expect("Messages key");
        let extracted = channel.extract_usage(UsageCtx {
            key,
            request_body: &runner.replay.body,
            response_headers: collected.headers(),
            response_body: collected.body(),
        });
        let estimated = extracted.is_none();
        let usage = extracted.unwrap_or_else(|| NormalizedUsage {
            input_tokens: crate::usage::estimate_input_tokens(&runner.replay.body),
            output_tokens: crate::usage::utf8_chars(collected.body()).div_ceil(2),
            ..Default::default()
        });
        let shaped = channel.shape_response(ResponseShapeCtx {
            key,
            status: collected.status(),
            headers: collected.headers(),
            body: collected.body(),
        });
        let body: Value = match shaped.map_err(CoreError::Channel).and_then(|body| {
            serde_json::from_slice(&body).map_err(|error| CoreError::Transform(error.to_string()))
        }) {
            Ok(body) => body,
            Err(error) => return failure(error),
        };
        let refused = body["stop_reason"] == "refusal";
        runner.meter.record(
            usage,
            runner.facts.target.upstream_model.clone(),
            runner.facts.upstream_started_at_ms.expect("send time"),
            refused,
            estimated,
        );
        runner.record_wire(&body);
        let (parts, _) = collected.into_parts();
        match runner.next(&body).await {
            Ok(Some(next)) => {
                previous = Some((parts, body));
                response = next;
            }
            Ok(None) => return finish(runner, parts, body).await,
            Err(error) => return failure(error),
        }
    }
}

pub(super) fn recommended(body: &mut Value, model: &str) {
    if body.get("stop_details").is_none_or(Value::is_null) {
        body["stop_details"] = json!({"type":"refusal","category":null,"explanation":null});
    }
    body["stop_details"]["recommended_model"] = json!(model);
}

async fn finish<H: Host>(
    runner: &Runner<H>,
    mut parts: http::response::Parts,
    body: Value,
) -> http::Response<ByteStream> {
    runner.pin(&body).await;
    parts.headers.remove(http::header::CONTENT_LENGTH);
    as_stream(http::Response::from_parts(
        parts,
        Bytes::from(runner.outward(body).to_string()),
    ))
}

pub(super) fn failure(error: CoreError) -> http::Response<ByteStream> {
    let mut response = http::Response::new(Bytes::from(error.body_json().to_string()));
    *response.status_mut() = error.status();
    response.headers_mut().insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/json"),
    );
    as_stream(response)
}

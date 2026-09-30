use gproxy_protocol::{
    HttpBody, WireRequest, WireResponse,
    adapt::embeddings::{
        EmbeddingBatchOptions, EmbeddingFailure, gemini_batch_to_openai, openai_to_gemini_batch,
    },
    capability::{
        CapabilityError, CapabilityFuture, CapabilityLimits, Upstream, UpstreamConnection,
    },
    codec::CodecLimits,
    connection::{Bytes, HeaderMap, Method, StatusCode},
    transform::{TransformErrorKind, embeddings::OpenAiUsageFacts},
    wire::openai::embeddings::CreateEmbeddingRequestBody,
};
use serde_json::{Value, json};

use std::{
    future::Future,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};

#[derive(Default)]
struct Host {
    sent: Mutex<Vec<Value>>,
    write_limit: Option<u64>,
    reject_call: Option<usize>,
    malformed_call: Option<usize>,
    omit_usage: bool,
    pending_second: bool,
    dropped: AtomicBool,
}
impl Upstream for Host {
    type Target = ();
    fn send<'a>(
        &'a self,
        _: &'a (),
        request: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse<HttpBody>, CapabilityError>> {
        Box::pin(async move {
            let HttpBody::Bytes(bytes) = request.body else {
                panic!("encoded body")
            };
            assert!(bytes.len() as u64 <= self.limits().write_bytes);
            let body: Value = serde_json::from_slice(&bytes).unwrap();
            let index = {
                let mut sent = self.sent.lock().unwrap();
                let index = sent.len();
                sent.push(body.clone());
                index
            };
            if self.pending_second && index == 1 {
                struct Dropped<'a>(&'a AtomicBool);
                impl Drop for Dropped<'_> {
                    fn drop(&mut self) {
                        self.0.store(true, Ordering::SeqCst);
                    }
                }
                let _guard = Dropped(&self.dropped);
                std::future::pending::<()>().await;
            }
            if self.reject_call == Some(index) {
                return Ok(WireResponse {
                    status: StatusCode::SERVICE_UNAVAILABLE,
                    headers: HeaderMap::new(),
                    body: HttpBody::Bytes(Bytes::from_static(b"upstream unavailable")),
                });
            }
            if let Some(input) = body.get("input") {
                let inputs: Vec<&Value> = match input {
                    Value::Array(values) => values.iter().collect(),
                    _ => vec![input],
                };
                let call_index = index;
                let dims = body["dimensions"].as_u64().unwrap_or(1) as usize;
                let data: Vec<_> = inputs.iter().enumerate().rev().map(|(index, value)| json!({
                    "object":"embedding", "index":if self.malformed_call == Some(call_index) { index + 1 } else { index },
                    "embedding":vec![value.as_str().unwrap().parse::<f64>().unwrap();dims], "unknown":true
                })).collect();
                let response = json!({"object":"list","model":"actual-openai-model","data":data,"usage":{"prompt_tokens":inputs.len(),"total_tokens":inputs.len()},"unknown":true});
                return Ok(WireResponse {
                    status: StatusCode::OK,
                    headers: HeaderMap::new(),
                    body: HttpBody::Bytes(Bytes::from(serde_json::to_vec(&response).unwrap())),
                });
            }
            if body.get("content").is_some() {
                let value = body["content"]["parts"][0]["text"]
                    .as_str()
                    .unwrap()
                    .parse::<f64>()
                    .unwrap();
                let response = json!({"embedding":{"values":[value],"unknown":true},"usageMetadata":{"promptTokenCount":1}});
                return Ok(WireResponse {
                    status: StatusCode::OK,
                    headers: HeaderMap::new(),
                    body: HttpBody::Bytes(Bytes::from(serde_json::to_vec(&response).unwrap())),
                });
            }
            let requests = body["requests"].as_array().unwrap();
            let embeddings: Vec<_> = if self.malformed_call == Some(index) {
                Vec::new()
            } else {
                requests.iter().map(|request| json!({"values":[request["content"]["parts"][0]["text"].as_str().unwrap().parse::<f64>().unwrap()], "unknown":true})).collect()
            };
            let mut response = json!({"embeddings":embeddings,"unknown":{"bad":true}});
            if !self.omit_usage {
                response["usageMetadata"] = json!({"promptTokenCount":requests.len()});
            }
            Ok(WireResponse {
                status: StatusCode::OK,
                headers: HeaderMap::new(),
                body: HttpBody::Bytes(Bytes::from(serde_json::to_vec(&response).unwrap())),
            })
        })
    }
    fn connect<'a>(
        &'a self,
        _: &'a (),
        _: WireRequest<()>,
    ) -> CapabilityFuture<'a, Result<UpstreamConnection, CapabilityError>> {
        panic!("unexpected WS")
    }
    fn limits(&self) -> CapabilityLimits {
        CapabilityLimits {
            operation_total: Duration::from_secs(30),
            stream_idle: Duration::from_secs(5),
            read_bytes: 4096,
            write_bytes: self.write_limit.unwrap_or(220),
            ws_frame_bytes: 1024,
        }
    }
}
fn template() -> WireRequest<()> {
    WireRequest {
        method: Method::POST,
        path: "/v1beta/models/embedding:batchEmbedContents".into(),
        query: None,
        headers: HeaderMap::new(),
        body: (),
    }
}
fn input() -> CreateEmbeddingRequestBody {
    serde_json::from_value(json!({"model":"client-alias","input":["1","2","3","4","5"],"dimensions":1,"encoding_format":"float","unknown":true})).unwrap()
}
fn options() -> EmbeddingBatchOptions {
    EmbeddingBatchOptions {
        codec: CodecLimits {
            max_body_bytes: 4096,
            max_buffer_bytes: 4096,
            max_value_bytes: 4096,
            max_line_bytes: 4096,
            max_part_bytes: 4096,
            max_parts: 8,
        },
        usage_per_call: Vec::new(),
    }
}
fn ready<F: Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);
    let mut cx = Context::from_waker(std::task::Waker::noop());
    match future.as_mut().poll(&mut cx) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("immediately-ready fixture expected"),
    }
}

#[test]
fn split_calls_preserve_input_order_global_indexes_model_and_actual_usage() {
    let host = Host::default();
    let result = ready(openai_to_gemini_batch(
        &host,
        &(),
        template(),
        input(),
        "embedding",
        options(),
    ))
    .unwrap();
    assert_eq!(result.completed_calls, 3);
    let wire = serde_json::to_value(result.output.value).unwrap();
    assert_eq!(wire["model"], "models/embedding");
    assert_eq!(wire["usage"], json!({"prompt_tokens":5,"total_tokens":5}));
    for index in 0..5 {
        assert_eq!(wire["data"][index]["index"], index);
        assert_eq!(
            wire["data"][index]["embedding"][0].as_f64(),
            Some((index + 1) as f64)
        );
    }
    assert!(wire.get("unknown").is_none());
    assert!(wire["data"][0].get("unknown").is_none());
    let sent = host.sent.lock().unwrap();
    assert_eq!(
        sent.iter()
            .map(|v| v["requests"].as_array().unwrap().len())
            .collect::<Vec<_>>(),
        vec![2, 2, 1]
    );
    assert_eq!(sent[0]["requests"][0]["model"], "models/embedding");
    assert!(sent[0].get("unknown").is_none());
}

#[test]
fn packing_obeys_host_byte_limit() {
    let host = Host {
        write_limit: Some(130),
        ..Host::default()
    };
    let result = ready(openai_to_gemini_batch(
        &host,
        &(),
        template(),
        input(),
        "embedding",
        options(),
    ))
    .unwrap();
    assert_eq!(result.completed_calls, 5);
    assert_eq!(result.output.value.data.len(), 5);
}

#[test]
fn failure_after_first_batch_is_never_partial_success_or_retried() {
    let host = Host {
        reject_call: Some(1),
        ..Host::default()
    };
    let error = ready(openai_to_gemini_batch(
        &host,
        &(),
        template(),
        input(),
        "embedding",
        options(),
    ))
    .unwrap_err();
    assert_eq!((error.completed_calls, error.completed_items), (1, 2));
    let EmbeddingFailure::Rejected(response) = error.failure else {
        panic!("HTTP failure expected")
    };
    assert_eq!(response.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(host.sent.lock().unwrap().len(), 2);
    let host = Host {
        malformed_call: Some(1),
        ..Host::default()
    };
    let error = ready(openai_to_gemini_batch(
        &host,
        &(),
        template(),
        input(),
        "embedding",
        options(),
    ))
    .unwrap_err();
    assert_eq!((error.completed_calls, error.completed_items), (2, 2));
    assert_eq!(host.sent.lock().unwrap().len(), 2);
}

#[test]
fn missing_usage_requires_actual_per_call_supplements() {
    let host = Host {
        omit_usage: true,
        ..Host::default()
    };
    let error = ready(openai_to_gemini_batch(
        &host,
        &(),
        template(),
        input(),
        "embedding",
        options(),
    ))
    .unwrap_err();
    let EmbeddingFailure::Transform(error) = error.failure else {
        panic!("missing metadata")
    };
    assert_eq!(error.kind(), TransformErrorKind::MissingMetadata);
    let host = Host {
        omit_usage: true,
        ..Host::default()
    };
    let mut options = options();
    options.usage_per_call = [2, 2, 1]
        .map(|n| {
            Some(OpenAiUsageFacts {
                prompt_tokens: n,
                total_tokens: n,
            })
        })
        .to_vec();
    let result = ready(openai_to_gemini_batch(
        &host,
        &(),
        template(),
        input(),
        "embedding",
        options,
    ))
    .unwrap();
    assert_eq!(result.output.value.usage.total_tokens, 5);
}

#[test]
fn dropping_inflight_batch_cancels_further_calls_without_retry() {
    let host = Host {
        pending_second: true,
        ..Host::default()
    };
    let mut future = Box::pin(openai_to_gemini_batch(
        &host,
        &(),
        template(),
        input(),
        "embedding",
        options(),
    ));
    let mut cx = Context::from_waker(std::task::Waker::noop());
    assert!(future.as_mut().poll(&mut cx).is_pending());
    assert_eq!(host.sent.lock().unwrap().len(), 2);
    drop(future);
    assert!(host.dropped.load(Ordering::SeqCst));
    assert_eq!(host.sent.lock().unwrap().len(), 2);
}

fn gemini_input() -> gproxy_protocol::wire::gemini::embeddings::BatchEmbedContentsRequestBody {
    serde_json::from_value(json!({"requests":[
        {"model":"models/source-a","content":{"parts":[{"text":"1","unknown":true}],"unknown":true},"outputDimensionality":1},
        {"model":"models/source-b","content":{"parts":[{"text":"2"}]},"outputDimensionality":2},
        {"model":"models/source-c","content":{"parts":[{"text":"3"}]},"outputDimensionality":2},
        {"model":"models/source-d","content":{"parts":[{"text":"4"}]},"outputDimensionality":1}
    ],"unknown":true})).unwrap()
}

#[test]
fn gemini_mixed_dimensions_split_and_reordered_openai_results_restore_input_order() {
    let host = Host::default();
    let mut endpoint = template();
    endpoint.path = "/v1/embeddings".into();
    let result = ready(gemini_batch_to_openai(
        &host,
        &(),
        endpoint,
        gemini_input(),
        "selected-openai",
        options(),
    ))
    .unwrap();
    assert_eq!(result.completed_calls, 3);
    let wire = serde_json::to_value(result.output.value).unwrap();
    assert_eq!(wire["usageMetadata"]["promptTokenCount"], 4);
    assert_eq!(wire["embeddings"].as_array().unwrap().len(), 4);
    for (index, dims) in [1, 2, 2, 1].into_iter().enumerate() {
        assert_eq!(
            wire["embeddings"][index]["values"]
                .as_array()
                .unwrap()
                .len(),
            dims
        );
        assert_eq!(
            wire["embeddings"][index]["values"][0].as_f64(),
            Some((index + 1) as f64)
        );
        assert!(wire["embeddings"][index].get("unknown").is_none());
    }
    let sent = host.sent.lock().unwrap();
    assert_eq!(
        sent.iter()
            .map(|v| v["input"].as_array().unwrap().len())
            .collect::<Vec<_>>(),
        vec![1, 2, 1]
    );
    assert!(sent.iter().all(|v| v["model"] == "selected-openai"));
}

#[test]
fn gemini_batch_omits_source_only_task_controls_and_executes_groups() {
    let host = Host::default();
    let mut input = gemini_input();
    input.requests.last_mut().unwrap().task_type =
        Some(gproxy_protocol::wire::gemini::embeddings::GeminiTaskType::RetrievalQuery);
    let output = ready(gemini_batch_to_openai(
        &host,
        &(),
        template(),
        input,
        "selected-openai",
        options(),
    ))
    .unwrap();
    assert_eq!(output.output.value.embeddings.unwrap().len(), 4);
    assert!(!host.sent.lock().unwrap().is_empty());
}

#[test]
fn single_calls_use_the_single_endpoint_and_preserve_encoding_and_dimensions() {
    use gproxy_protocol::adapt::embeddings::{gemini_single_to_openai, openai_to_gemini_single};
    let host = Host::default();
    let input = serde_json::from_value(
        json!({"model":"source","input":"1","dimensions":1,"encoding_format":"base64"}),
    )
    .unwrap();
    let mut endpoint = template();
    endpoint.path = "/v1beta/models/embedding:embedContent".into();
    let result = ready(openai_to_gemini_single(
        &host,
        &(),
        endpoint,
        input,
        "embedding",
        options().codec,
        None,
    ))
    .unwrap();
    let wire = serde_json::to_value(result.output.value).unwrap();
    assert_eq!(wire["data"][0]["embedding"], "AACAPw==");
    assert_eq!(result.completed_calls, 1);
    assert!(host.sent.lock().unwrap()[0].get("content").is_some());
    let host = Host::default();
    let input = serde_json::from_value(
        json!({"content":{"parts":[{"text":"2"}]},"outputDimensionality":2}),
    )
    .unwrap();
    let mut endpoint = template();
    endpoint.path = "/v1/embeddings".into();
    let result = ready(gemini_single_to_openai(
        &host,
        &(),
        endpoint,
        input,
        "selected-openai",
        options().codec,
    ))
    .unwrap();
    let wire = serde_json::to_value(result.output.value).unwrap();
    assert_eq!(wire["embedding"]["values"].as_array().unwrap().len(), 2);
    assert_eq!(wire["embedding"]["values"][0].as_f64(), Some(2.0));
    assert_eq!(wire["usageMetadata"]["promptTokenCount"], 1);
    assert_eq!(host.sent.lock().unwrap()[0]["input"], "2");
}

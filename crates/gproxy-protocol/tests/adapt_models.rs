use gproxy_protocol::{
    HttpBody, WireRequest, WireResponse,
    adapt::models::*,
    capability::{
        CapabilityError, CapabilityFuture, CapabilityLimits, Upstream, UpstreamConnection,
    },
    codec::CodecLimits,
    connection::{Bytes, HeaderMap, Method, StatusCode},
    transform::TransformErrorKind,
    wire::{claude::models as c, gemini::models as g},
};
use serde_json::json;
use std::{
    future::Future,
    sync::Mutex,
    task::{Context, Poll},
    time::Duration,
};

struct Host {
    sent: Mutex<Vec<WireRequest<HttpBody>>>,
    response: Mutex<std::collections::VecDeque<WireResponse<HttpBody>>>,
    pending_at: Option<usize>,
    read_limit: u64,
    write_limit: u64,
}
impl Upstream for Host {
    type Target = ();
    fn send<'a>(
        &'a self,
        _: &'a (),
        request: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse<HttpBody>, CapabilityError>> {
        Box::pin(async move {
            let call = {
                let mut sent = self.sent.lock().unwrap();
                let call = sent.len();
                sent.push(request);
                call
            };
            if self.pending_at == Some(call) {
                std::future::pending::<()>().await;
            }
            Ok(self
                .response
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected additional call"))
        })
    }
    fn connect<'a>(
        &'a self,
        _: &'a (),
        _: WireRequest<()>,
    ) -> CapabilityFuture<'a, Result<UpstreamConnection, CapabilityError>> {
        panic!("unexpected WS call")
    }
    fn limits(&self) -> CapabilityLimits {
        CapabilityLimits {
            operation_total: Duration::from_secs(30),
            stream_idle: Duration::from_secs(5),
            read_bytes: self.read_limit,
            write_bytes: self.write_limit,
            ws_frame_bytes: 1024,
        }
    }
}
fn host(status: StatusCode, body: HttpBody) -> Host {
    Host {
        sent: Mutex::default(),
        response: Mutex::new(std::collections::VecDeque::from([WireResponse {
            status,
            headers: HeaderMap::new(),
            body,
        }])),
        pending_at: None,
        read_limit: 1024,
        write_limit: 1024,
    }
}

fn limits() -> ModelListLimits {
    ModelListLimits {
        codec: CodecLimits {
            max_buffer_bytes: 100_000,
            max_value_bytes: 100_000,
            max_body_bytes: 100_000,
            max_line_bytes: 100_000,
            max_part_bytes: 100_000,
            max_parts: 8,
        },

        max_declared_bytes: 100_000,
    }
}
fn request() -> WireRequest<()> {
    WireRequest {
        method: Method::GET,
        path: "/v1/models".into(),
        query: None,
        headers: HeaderMap::new(),
        body: (),
    }
}
fn ready<F: Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);
    let mut cx = Context::from_waker(std::task::Waker::noop());
    match future.as_mut().poll(&mut cx) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("fixture should be immediately ready"),
    }
}
fn pages(values: Vec<serde_json::Value>) -> Host {
    let mut host = host(StatusCode::OK, HttpBody::Bytes(Bytes::new()));
    host.read_limit = 100_000;
    host.response = Mutex::new(
        values
            .into_iter()
            .map(|value| WireResponse {
                status: StatusCode::OK,
                headers: HeaderMap::new(),
                body: HttpBody::Bytes(serde_json::to_vec(&value).unwrap().into()),
            })
            .collect(),
    );
    host
}
fn gm(name: &str) -> serde_json::Value {
    json!({"name":name,"baseModelId":"b","version":"v","extension":{"secret":true}})
}
fn cm(id: &str) -> serde_json::Value {
    let support = json!({"supported":false,"secret":true});
    json!({"id":id,"allowed_fallback_models":[],"created_at":"2024-01-01T00:00:00Z","display_name":id,"max_input_tokens":1000,"max_tokens":100,"type":"model","extension":true,
      "capabilities":{"batch":support,"citations":support,"code_execution":support,"image_input":support,"pdf_input":support,"structured_outputs":support,
      "context_management":{"supported":false,"clear_thinking_20251015":support,"clear_tool_uses_20250919":support,"compact_20260112":support},
      "effort":{"supported":false,"high":support,"low":support,"max":support,"medium":support,"xhigh":support},
      "thinking":{"supported":false,"types":{"adaptive":support,"enabled":support}}}})
}
#[test]
fn gemini_pages_encode_opaque_tokens_and_convert_complete_directory() {
    use gproxy_protocol::transform::models::OpenAiModelSupplement;
    let host = pages(vec![
        json!({"models":[gm("models/a")],"nextPageToken":"a+/=&雪"}),
        json!({"models":[gm("models/b")],"nextPageToken":""}),
    ]);
    let supplements = std::collections::BTreeMap::from([
        (
            "models/a".into(),
            OpenAiModelSupplement {
                owned_by: "org".into(),
                created: Some(1),
            },
        ),
        (
            "models/b".into(),
            OpenAiModelSupplement {
                owned_by: "org".into(),
                created: Some(2),
            },
        ),
    ]);
    let result = ready(gemini_to_openai_list(
        &host,
        &(),
        request(),
        g::ListModelsQuery::builder().page_size(7).build(),
        &supplements,
        limits(),
    ))
    .unwrap();
    assert_eq!(result.completed_calls, 2);
    assert_eq!(
        result
            .value
            .value
            .data
            .iter()
            .map(|m| m.id.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "b"]
    );
    assert!(result.value.value.data.iter().all(|m| m.rest.is_empty()));
    let sent = host.sent.lock().unwrap();
    assert_eq!(sent[0].query.as_deref(), Some("pageSize=7"));
    assert_eq!(
        sent[1].query.as_deref(),
        Some("pageSize=7&pageToken=a%2B%2F%3D%26%E9%9B%AA")
    );
    assert!(
        sent.iter()
            .all(|r| matches!(&r.body, HttpBody::Bytes(b) if b.is_empty()))
    );
}
#[test]
fn claude_pages_preserve_boundaries_and_clear_nested_extensions() {
    let host = pages(vec![
        json!({"data":[cm("a")],"first_id":"a","last_id":"a","has_more":true}),
        json!({"data":[cm("b")],"first_id":"b","last_id":"b","has_more":false}),
    ]);
    let result = ready(collect_claude(
        &host,
        &(),
        request(),
        c::ListModelsQuery::builder().limit(1).build(),
        limits(),
    ))
    .unwrap();
    assert_eq!(result.value.first_id, "a");
    assert_eq!(result.value.last_id, "b");
    assert!(!result.value.has_more);
    assert!(result.value.data[0].capabilities.batch.rest.is_empty());
    assert_eq!(
        host.sent.lock().unwrap()[1].query.as_deref(),
        Some("limit=1&after_id=a")
    );
}

#[test]
fn catalogues_accept_omitted_fallbacks_and_model_version() {
    let mut model = cm("claude-sonnet-4-6");
    model
        .as_object_mut()
        .unwrap()
        .remove("allowed_fallback_models");
    let host = pages(vec![json!({
        "data": [model], "first_id": "claude-sonnet-4-6",
        "last_id": "claude-sonnet-4-6", "has_more": false
    })]);
    let result = ready(collect_claude(
        &host,
        &(),
        request(),
        c::ListModelsQuery::builder().build(),
        limits(),
    ))
    .unwrap();
    assert!(result.value.data[0].allowed_fallback_models.is_empty());

    let host = pages(vec![json!({"models": [{
        "name": "models/gemini-3-flash", "baseModelId": "gemini-3-flash",
        "supportedGenerationMethods": ["generateContent", "streamGenerateContent"]
    }]})]);
    let result = ready(collect_gemini(
        &host,
        &(),
        request(),
        g::ListModelsQuery::builder().build(),
        limits(),
    ))
    .unwrap();
    let model = &result.value.models.as_ref().unwrap()[0];
    assert_eq!(model.name, "models/gemini-3-flash");
    assert!(model.version.is_empty());
    assert!(
        serde_json::to_value(model)
            .unwrap()
            .get("version")
            .is_none()
    );
}

#[test]
fn paging_cycles_fail_without_partial_success() {
    let host = pages(vec![
        json!({"nextPageToken":"cycle"}),
        json!({"nextPageToken":"cycle"}),
    ]);
    let err = ready(collect_gemini(
        &host,
        &(),
        request(),
        g::ListModelsQuery::builder().build(),
        limits(),
    ))
    .unwrap_err();
    assert_eq!(err.completed_calls, 2);
    assert_eq!(host.sent.lock().unwrap().len(), 2);
    let host = pages(vec![
        json!({"models":[gm("models/a")],"nextPageToken":"x"}),
        json!({"models":[gm("models/a")]}),
    ]);
    assert!(
        ready(collect_gemini(
            &host,
            &(),
            request(),
            g::ListModelsQuery::builder().build(),
            limits()
        ))
        .is_err()
    );
}
#[test]
fn initial_cursor_is_not_mistaken_for_a_complete_directory_and_limits_bound_retention() {
    let host = pages(vec![]);
    assert!(
        ready(collect_gemini(
            &host,
            &(),
            request(),
            g::ListModelsQuery::builder().page_token("middle").build(),
            limits()
        ))
        .is_err()
    );
    assert!(host.sent.lock().unwrap().is_empty());
    {
        let host = pages(vec![json!({"models":[gm("models/a")]})]);
        let mut bound = limits();
        bound.max_declared_bytes = 1;
        let err = ready(collect_gemini(
            &host,
            &(),
            request(),
            g::ListModelsQuery::builder().build(),
            bound,
        ))
        .unwrap_err();
        let ModelListFailure::Transform(error) = err.failure else {
            panic!()
        };
        assert_eq!(error.kind(), TransformErrorKind::Limit);
    }
}
#[test]
fn second_http_failure_preserves_raw_body_and_does_not_retry() {
    let host = pages(vec![json!({"nextPageToken":"next"})]);
    host.response.lock().unwrap().push_back(WireResponse {
        status: StatusCode::SERVICE_UNAVAILABLE,
        headers: HeaderMap::new(),
        body: HttpBody::Stream(Box::pin(futures_util::stream::poll_fn(|_| {
            panic!("failure body must not be consumed")
        }))),
    });
    let err = ready(collect_gemini(
        &host,
        &(),
        request(),
        g::ListModelsQuery::builder().build(),
        limits(),
    ))
    .unwrap_err();
    assert_eq!(err.completed_calls, 1);
    let ModelListFailure::Rejected(r) = err.failure else {
        panic!()
    };
    assert_eq!(r.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(host.sent.lock().unwrap().len(), 2);
}
#[test]
fn inconsistent_claude_boundaries_and_missing_mapping_facts_fail_explicitly() {
    let host = pages(vec![
        json!({"data":[cm("a")],"first_id":"wrong","last_id":"a","has_more":false}),
    ]);
    assert!(
        ready(collect_claude(
            &host,
            &(),
            request(),
            c::ListModelsQuery::builder().build(),
            limits()
        ))
        .is_err()
    );
    let host = pages(vec![json!({"models":[gm("models/a")]})]);
    let err = ready(gemini_to_openai_list(
        &host,
        &(),
        request(),
        g::ListModelsQuery::builder().build(),
        &std::collections::BTreeMap::new(),
        limits(),
    ))
    .unwrap_err();
    assert_eq!(err.completed_calls, 1);
    assert_eq!(err.completed_models, 1);
    let ModelListFailure::Transform(e) = err.failure else {
        panic!()
    };
    assert_eq!(e.kind(), TransformErrorKind::MissingMetadata);
}

#[test]
fn dropping_a_pending_second_page_stops_further_collection() {
    let mut host = pages(vec![
        json!({"models":[gm("models/a")],"nextPageToken":"next"}),
    ]);
    host.pending_at = Some(1);
    let mut future = Box::pin(collect_gemini(
        &host,
        &(),
        request(),
        g::ListModelsQuery::builder().build(),
        limits(),
    ));
    let mut cx = Context::from_waker(std::task::Waker::noop());
    assert!(future.as_mut().poll(&mut cx).is_pending());
    assert_eq!(host.sent.lock().unwrap().len(), 2);
    drop(future);
    assert_eq!(host.sent.lock().unwrap().len(), 2);
}
#[test]
fn openai_unpaged_directory_is_called_once_and_uses_real_gemini_facts() {
    use gproxy_protocol::transform::models::GeminiModelSupplement;
    let host = pages(vec![
        json!({"object":"list","data":[{"id":"a","object":"model","created":1,"owned_by":"org","extension":true}]}),
    ]);
    let facts = std::collections::BTreeMap::from([(
        "a".into(),
        GeminiModelSupplement {
            base_model_id: "base".into(),
            version: "v1".into(),
        },
    )]);
    let result = ready(openai_to_gemini_list(
        &host,
        &(),
        request(),
        &facts,
        limits(),
    ))
    .unwrap();
    assert_eq!(result.completed_calls, 1);
    let model = &result.value.value.models.as_ref().unwrap()[0];
    assert_eq!(model.name, "models/a");
    assert_eq!(model.base_model_id, "base");
    assert_eq!(model.version, "v1");
    assert!(model.rest.is_empty());
}

#[test]
fn empty_terminal_claude_page_keeps_last_collected_model_boundary() {
    let host = pages(vec![
        json!({"data":[cm("a")],"first_id":"a","last_id":"a","has_more":true}),
        json!({"data":[],"first_id":"","last_id":"","has_more":false}),
    ]);
    let result = ready(collect_claude(
        &host,
        &(),
        request(),
        c::ListModelsQuery::builder().build(),
        limits(),
    ))
    .unwrap();
    assert_eq!(result.completed_calls, 2);
    assert_eq!(result.value.data.len(), 1);
    assert_eq!(result.value.first_id, "a");
    assert_eq!(result.value.last_id, "a");
}

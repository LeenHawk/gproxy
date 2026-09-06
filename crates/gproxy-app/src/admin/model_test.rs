use bytes::Bytes;
use gproxy_admin::AdminError;
use gproxy_admin::dto::{ModelTestRequest, ModelTestResponse};
use http::{HeaderMap, HeaderValue, Method};
use serde_json::json;
use web_time::Instant;

use crate::AppHandle;

/// One small completion, sent the way a client would send it.
///
/// This is real inference: it authenticates with the operator's own key, passes
/// admission and quota, and settles like any other request. There is no cheaper
/// path on purpose — a probe that skipped the funnel would be traffic nobody paid
/// for, which is the hole v2 opened with its service bypass.
pub(super) async fn run(
    app: &AppHandle,
    actor_user_id: i64,
    request: &ModelTestRequest,
) -> Result<ModelTestResponse, AdminError> {
    let snapshot = app.inner.host.services.control.current();
    let provider = snapshot
        .providers
        .iter()
        .find(|provider| provider.id == request.provider_id)
        .ok_or_else(|| AdminError::BadRequest("unknown provider".into()))?;

    let (key_prefix, secret) = super::operator_key(app, actor_user_id, &snapshot).await?;

    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(
        http::header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {secret}"))
            .map_err(|_| AdminError::Internal("malformed test key".into()))?,
    );
    let body = json!({
        "model": request.model_id,
        "max_tokens": 16,
        "messages": [{ "role": "user", "content": "hello" }],
    });
    let ctx = gproxy_core::RequestCtx {
        request_id: format!("model-test:{}:{}", request.provider_id, request.model_id),
        client_ip: None,
        method: Method::POST,
        // The provider prefix lives in the routing mode, not the path: ingress patterns
        // are matched after a host has stripped it.
        path: "/v1/chat/completions".into(),
        query: None,
        headers,
        body: Bytes::from(serde_json::to_vec(&body).expect("static json")),
        upgrade: false,
        force_model_refresh: false,
        mode: gproxy_core::RoutingMode::Scoped {
            provider: provider.name.clone(),
        },
    };

    let started = Instant::now();
    let outcome = app.execute(ctx).await;
    let latency_ms = started.elapsed().as_millis().try_into().unwrap_or(u64::MAX);

    match outcome {
        Ok(outcome) => {
            let status = outcome.status.as_u16();
            let ok = outcome.status.is_success();
            let body = match outcome.body {
                gproxy_core::ResponseBody::Full(bytes) => Some(bytes),
                _ => None,
            };
            Ok(ModelTestResponse {
                ok,
                status,
                latency_ms,
                key_prefix,
                reply: ok.then(|| body.as_deref().and_then(first_text)).flatten(),
                // What the upstream said about the refusal is the answer the operator
                // came for, so it is carried through rather than reduced to a status.
                message: (!ok)
                    .then(|| body.as_deref().and_then(upstream_error))
                    .flatten(),
            })
        }
        Err(error) => Ok(ModelTestResponse {
            ok: false,
            status: 0,
            latency_ms,
            key_prefix,
            reply: None,
            message: Some(error.to_string()),
        }),
    }
}

/// The first assistant text in an OpenAI-shaped reply, trimmed for display.
fn first_text(body: &[u8]) -> Option<String> {
    let value = serde_json::from_slice::<serde_json::Value>(body).ok()?;
    let text = value
        .get("choices")?
        .as_array()?
        .first()?
        .get("message")?
        .get("content")?
        .as_str()?;
    Some(text.chars().take(200).collect())
}

/// An upstream refusal, in whatever shape the vendor sent it: OpenAI and Claude both
/// nest a message under `error`, and anything else falls back to the raw body.
fn upstream_error(body: &[u8]) -> Option<String> {
    let text = serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            let error = value.get("error")?;
            error
                .get("message")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
                .or_else(|| Some(error.to_string()))
        })
        .or_else(|| String::from_utf8(body.to_vec()).ok())?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.chars().take(300).collect())
}

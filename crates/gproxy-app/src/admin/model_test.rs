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

    // The oldest enabled key the operator owns: stable, and on a fresh instance it
    // is the one bootstrap minted, so the button works before anything is configured.
    let key = snapshot
        .user_keys
        .iter()
        .filter(|key| key.user_id == actor_user_id && key.enabled)
        .min_by_key(|key| key.id)
        .ok_or_else(|| {
            AdminError::BadRequest("this administrator has no enabled API key to test with".into())
        })?;
    let stored = app
        .inner
        .host
        .services
        .store
        .user_key_secret(key.id)
        .await?
        .and_then(|secret| secret.envelope)
        .ok_or_else(|| AdminError::Conflict("the test key predates revealable storage".into()))?;
    let secret = app
        .inner
        .host
        .services
        .cipher
        .open_user_key(&stored)
        .map_err(|error| AdminError::Internal(error.to_string()))?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| AdminError::Internal("stored key is not a string".into()))?;

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
        method: Method::POST,
        path: format!("/{}/v1/chat/completions", provider.name),
        query: None,
        headers,
        body: Bytes::from(serde_json::to_vec(&body).expect("static json")),
        upgrade: false,
        mode: gproxy_core::RoutingMode::Scoped {
            provider: provider.name.clone(),
        },
    };

    let started = Instant::now();
    let outcome = app.execute(ctx).await;
    let latency_ms = started.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
    let key_prefix = key.prefix.clone().unwrap_or_else(|| format!("#{}", key.id));

    match outcome {
        Ok(outcome) => {
            let status = outcome.status.as_u16();
            let reply = match outcome.body {
                gproxy_core::ResponseBody::Full(bytes) => first_text(&bytes),
                _ => None,
            };
            Ok(ModelTestResponse {
                ok: outcome.status.is_success(),
                status,
                latency_ms,
                key_prefix,
                reply,
                message: None,
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

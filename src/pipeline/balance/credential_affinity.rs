//! Hard credential bindings for upstream state owned by one subscription account.

use std::sync::Arc;
use std::time::Duration;

use http::HeaderMap;

use crate::pipeline::context::{Candidate, RequestCtx};
use crate::protocol::Operation;
use crate::store::cache::CacheBackend;

const BINDING_TTL: Duration = Duration::from_secs(24 * 3600);
const TURN_STATE_HEADER: &str = "x-codex-turn-state";

pub(crate) fn request_key(ctx: &RequestCtx, user_key_id: i64) -> Option<Arc<str>> {
    if let Some(state) = header(&ctx.headers, TURN_STATE_HEADER) {
        return Some(key("turn", user_key_id, state));
    }
    match ctx.op?.operation() {
        Operation::WebSearch => body_string(&ctx.body, "id")
            .filter(|id| !id.is_empty())
            .map(|id| key("search", user_key_id, &id)),
        Operation::ConnectRealtime => query_value(ctx.query.as_deref(), "call_id")
            .filter(|id| !id.is_empty())
            .map(|id| key("realtime", user_key_id, &id)),
        _ => None,
    }
}

pub(crate) async fn record_response(
    cache: &dyn CacheBackend,
    ctx: &RequestCtx,
    headers: &HeaderMap,
    candidate: &Candidate,
) {
    let Some(user_key_id) = ctx.identity.as_ref().map(|id| id.user_key.id) else {
        return;
    };
    if let Some(state) = header(headers, TURN_STATE_HEADER) {
        write(
            cache,
            &key("turn", user_key_id, state),
            candidate.credential.id,
        )
        .await;
    }
    if ctx
        .op
        .is_some_and(|op| op.operation() == Operation::CreateRealtimeCall)
        && let Some(call_id) = headers
            .get(http::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .and_then(call_id_from_location)
    {
        write(
            cache,
            &key("realtime", user_key_id, call_id),
            candidate.credential.id,
        )
        .await;
        if let Some(model) = ctx.route_name.as_deref().or(ctx.body_model.as_deref()) {
            let _ = cache
                .set(
                    &model_key(user_key_id, call_id),
                    model.as_bytes().to_vec(),
                    Some(BINDING_TTL),
                )
                .await;
        }
    }
}

pub(crate) async fn realtime_model(
    cache: &dyn CacheBackend,
    user_key_id: i64,
    query: Option<&str>,
) -> Option<String> {
    let call_id = query_value(query, "call_id")?;
    cache
        .get(&model_key(user_key_id, &call_id))
        .await
        .and_then(|value| String::from_utf8(value).ok())
        .filter(|value| !value.is_empty())
}

fn key(kind: &str, user_key_id: i64, value: &str) -> Arc<str> {
    format!(
        "cred_bind:{kind}:{user_key_id}:{}",
        blake3::hash(value.as_bytes()).to_hex()
    )
    .into()
}

fn model_key(user_key_id: i64, call_id: &str) -> String {
    format!(
        "realtime_model:{user_key_id}:{}",
        blake3::hash(call_id.as_bytes()).to_hex()
    )
}

pub(crate) async fn read(cache: &dyn CacheBackend, key: Option<&str>) -> Option<i64> {
    let key = key?;
    cache
        .get(key)
        .await
        .and_then(|value| String::from_utf8(value).ok())
        .and_then(|value| value.parse().ok())
}

pub(crate) async fn write(cache: &dyn CacheBackend, key: &str, credential_id: i64) {
    let _ = cache
        .set(
            key,
            credential_id.to_string().into_bytes(),
            Some(BINDING_TTL),
        )
        .await;
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get(name)?
        .to_str()
        .ok()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn body_string(body: &[u8], field: &str) -> Option<String> {
    serde_json::from_slice::<serde_json::Value>(body)
        .ok()?
        .get(field)?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn query_value(query: Option<&str>, name: &str) -> Option<String> {
    serde_urlencoded::from_str::<Vec<(String, String)>>(query?)
        .ok()?
        .into_iter()
        .find_map(|(key, value)| (key == name && !value.is_empty()).then_some(value))
}

fn call_id_from_location(location: &str) -> Option<&str> {
    location
        .split(['?', '#'])
        .next()?
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|value| !value.is_empty())
}

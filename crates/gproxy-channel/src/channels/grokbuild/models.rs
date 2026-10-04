//! Model capability facts from the assigned account's catalogue.

use super::{GrokBuild, config::base_url, unix_now_ms};
use crate::channel::{
    BaseChannel, ChannelState, OperationContext, OperationFuture, PrepareContext, ProviderView,
};
use crate::channels::shared::compatible::http::read_body;
use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireRequest, WireResponse, capability::StateWrite,
    connection::Bytes,
};
use http::{HeaderMap, Method};
use serde_json::Value;

fn supports_effort(catalog: &Value, model: &str) -> Option<bool> {
    let entry = catalog
        .get("data")?
        .as_array()?
        .iter()
        .find(|entry| entry.get("id").and_then(Value::as_str) == Some(model))?;
    let menu = entry
        .get("reasoningEfforts")
        .or_else(|| entry.get("reasoning_efforts"))
        .or_else(|| entry.pointer("/_meta/reasoningEfforts"))
        .or_else(|| entry.pointer("/capabilities/reasoning_effort"));
    if menu
        .and_then(Value::as_array)
        .is_some_and(|menu| !menu.is_empty())
    {
        return Some(true);
    }
    Some(
        entry
            .get("supportsReasoningEffort")
            .or_else(|| entry.get("supports_reasoning_effort"))
            .or_else(|| entry.pointer("/_meta/supportsReasoningEffort"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
    )
}

async fn cache(provider: ProviderView<'_>, state: &dyn ChannelState, body: &Bytes) {
    let key = format!("grokbuild:models:{}", base_url(provider));
    let expected = state
        .get(&key)
        .await
        .ok()
        .flatten()
        .map(|entry| entry.version);
    let expires_at = std::time::UNIX_EPOCH
        + std::time::Duration::from_millis(unix_now_ms().max(0) as u64 + 300_000);
    let _ = state
        .compare_exchange(
            &key,
            expected,
            Some(StateWrite {
                payload: body.clone(),
                expires_at: Some(expires_at),
            }),
        )
        .await;
}

pub(super) fn list<'a>(
    channel: &'a GrokBuild,
    context: OperationContext<'a>,
) -> OperationFuture<'a, WireResponse<HttpBody>> {
    Box::pin(async move {
        let request = channel.prepare(PrepareContext {
            provider: context.provider,
            credential: context.credential,
            operation: OperationKey {
                operation: Operation::ListModels,
                dialect: context.dialect,
            },
            request: context.request,
            endpoint_override: context.endpoint_override,
        })?;
        let response = context.client.send(request).await?;
        let body = read_body(response.body).await?;
        if response.status.is_success()
            && serde_json::from_slice::<Value>(&body)
                .ok()
                .is_some_and(|value| value.get("data").is_some_and(Value::is_array))
        {
            cache(context.provider, context.state.as_ref(), &body).await;
        }
        Ok(WireResponse {
            status: response.status,
            headers: response.headers,
            body: HttpBody::Bytes(body),
        })
    })
}

pub(super) fn generate<'a>(
    channel: &'a GrokBuild,
    operation: Operation,
    mut context: OperationContext<'a>,
) -> OperationFuture<'a, WireResponse<HttpBody>> {
    Box::pin(async move {
        if context.dialect == Dialect::OpenAi
            && let HttpBody::Bytes(body) = &context.request.body
            && let Ok(mut value) = serde_json::from_slice::<Value>(body)
            && value.pointer("/reasoning/effort").is_some()
            && let Some(model) = value.get("model").and_then(Value::as_str)
        {
            let key = format!("grokbuild:models:{}", base_url(context.provider));
            let mut catalog = context
                .state
                .get(&key)
                .await
                .ok()
                .flatten()
                .and_then(|entry| serde_json::from_slice::<Value>(&entry.payload).ok());
            if catalog.is_none() {
                let request = channel.prepare(PrepareContext {
                    provider: context.provider,
                    credential: context.credential,
                    operation: OperationKey {
                        operation: Operation::ListModels,
                        dialect: Dialect::OpenAi,
                    },
                    request: WireRequest {
                        method: Method::GET,
                        path: "/v1/models".into(),
                        query: None,
                        headers: HeaderMap::new(),
                        body: HttpBody::Bytes(Bytes::new()),
                    },
                    endpoint_override: None,
                })?;
                // Catalogue availability must not prevent generation. Unknown models keep the caller's effort.
                if let Ok(response) = context.client.send(request).await
                    && response.status.is_success()
                    && let Ok(body) = read_body(response.body).await
                    && let Ok(parsed) = serde_json::from_slice::<Value>(&body)
                    && parsed.get("data").is_some_and(Value::is_array)
                {
                    cache(context.provider, context.state.as_ref(), &body).await;
                    catalog = Some(parsed);
                }
            }
            if catalog
                .as_ref()
                .and_then(|catalog| supports_effort(catalog, model))
                == Some(false)
                && let Some(reasoning) = value.get_mut("reasoning").and_then(Value::as_object_mut)
            {
                reasoning.remove("effort");
                if reasoning.is_empty() {
                    value.as_object_mut().unwrap().remove("reasoning");
                }
                context.request.body = HttpBody::Bytes(Bytes::from(value.to_string()));
            }
        }
        let request = channel.prepare(PrepareContext {
            provider: context.provider,
            credential: context.credential,
            operation: OperationKey {
                operation,
                dialect: context.dialect,
            },
            request: context.request,
            endpoint_override: context.endpoint_override,
        })?;
        Ok(context.client.send(request).await?)
    })
}

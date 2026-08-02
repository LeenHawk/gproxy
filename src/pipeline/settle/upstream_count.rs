//! Native upstream token-count rung used by the settlement ladder.

use bytes::Bytes;
use serde_json::{Value, json};

use super::SettleCtx;
use crate::protocol::{Operation, OperationKey, Provider as Family};
use crate::usage::NormalizedUsage;

static COUNT_GATE: std::sync::OnceLock<tokio::sync::Semaphore> = std::sync::OnceLock::new();
const COUNT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

pub(super) async fn count(ctx: &SettleCtx, text: &str) -> Result<NormalizedUsage, &'static str> {
    let gate = COUNT_GATE.get_or_init(|| tokio::sync::Semaphore::new(4));
    let _permit = gate.acquire().await.map_err(|_| "concurrency_gate")?;
    let secret = ctx
        .state
        .cipher
        .open(&ctx.credential.secret_json)
        .map_err(|_| "secret_open")?;
    let input = count_once(ctx, &secret, ctx.request_body.clone()).await?;
    let output = if text.is_empty() {
        0
    } else {
        count_once(
            ctx,
            &secret,
            output_count_body(ctx.upstream_family, &ctx.model, text),
        )
        .await?
    };
    Ok(NormalizedUsage {
        input,
        output,
        ..Default::default()
    })
}

async fn count_once(ctx: &SettleCtx, secret: &Value, body: Bytes) -> Result<u64, &'static str> {
    let key = OperationKey::provider(Operation::CountTokens, ctx.upstream_family);
    let target = crate::protocol::request_target(key, &ctx.model, false);
    let mut headers = http::HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/json"),
    );
    let prepared = ctx
        .channel
        .prepare(crate::channel::PrepareCtx {
            secret,
            provider_settings: &ctx.provider.settings_json,
            op: key,
            stream: false,
            upstream_model_id: &ctx.model,
            method: target.method.into(),
            path: &target.path,
            query: target.query.as_deref(),
            headers: &headers,
            body,
        })
        .map_err(|_| "prepare")?;
    let client = ctx
        .state
        .upstream_client_for_credential(&ctx.channel, &ctx.credential, &ctx.provider)
        .map_err(|_| "client_resolve")?;
    let resp = tokio::time::timeout(COUNT_TIMEOUT, prepared.send_buffered(client))
        .await
        .map_err(|_| "timeout")?
        .map_err(|_| "transport")?;
    if !resp.status().is_success() {
        return Err("non_success_status");
    }
    let value: Value = serde_json::from_slice(resp.body()).map_err(|_| "response_parse")?;
    value
        .get("input_tokens")
        .or_else(|| value.get("totalTokens"))
        .and_then(Value::as_u64)
        .ok_or("token_count_missing")
}

fn output_count_body(family: Family, model: &str, text: &str) -> Bytes {
    let value = match family {
        Family::Claude => json!({"model": model, "messages": [{"role": "user", "content": text}]}),
        Family::Gemini => json!({"contents": [{"role": "user", "parts": [{"text": text}]}]}),
        Family::OpenAi => json!({"model": model, "input": text}),
    };
    Bytes::from(serde_json::to_vec(&value).expect("json! serializes"))
}

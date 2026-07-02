//! §17 counting ladder for abnormal/usage-less ends: gpt family → local
//! tiktoken; claude/gemini upstream family → upstream count endpoint (global
//! Semaphore(4) + 5s timeout, same effective provider client, no user quota/authz);
//! anything else / failure → local chain (vocab → chars/2).

#[cfg(not(target_arch = "wasm32"))]
use bytes::Bytes;
use serde_json::{Value, json};

use super::{SettleCtx, record};
use crate::app::AppState;
#[cfg(not(target_arch = "wasm32"))]
use crate::protocol::Provider as Family;
use crate::usage::{Ended, NormalizedUsage, UsageSource};

pub(super) async fn count_and_record(ctx: SettleCtx, text: String, ended: Ended) {
    let (usage, source) = ladder(&ctx, &text).await;
    record(&ctx, usage, source, ended).await;
}

/// §17 counting ladder: gpt family → local tiktoken; claude/gemini upstream
/// family → upstream count endpoint (bounded concurrency + timeout, same
/// effective provider client, never the user pipeline); anything else / failure
/// → local chain (vocab → chars/2).
pub(super) async fn ladder(ctx: &SettleCtx, text: &str) -> (NormalizedUsage, UsageSource) {
    #[cfg(not(target_arch = "wasm32"))]
    if !crate::tokenize::is_gpt_family(&ctx.model)
        && matches!(ctx.upstream_family, Family::Claude | Family::Gemini)
        && let Some(u) = upstream_count(ctx, text).await
    {
        return (u, UsageSource::Counted);
    }
    local_ladder(ctx, text)
}

/// Local chain: input from the captured request body, output from the
/// produced text wrapped as a single user message.
fn local_ladder(ctx: &SettleCtx, text: &str) -> (NormalizedUsage, UsageSource) {
    let map = ctx.provider.settings_json.get("tokenizer_map");
    let input = local_count(&ctx.state, &ctx.model, map, &ctx.request_body);
    let output = if text.is_empty() {
        0
    } else {
        let body = serde_json::to_vec(&json!({
            "messages": [{ "role": "user", "content": text }]
        }))
        .unwrap_or_default();
        local_count(&ctx.state, &ctx.model, map, &body)
    };
    // tiktoken is exact for gpt families; everything else is an estimate
    let source = if cfg!(feature = "count-local") && crate::tokenize::is_gpt_family(&ctx.model) {
        UsageSource::Counted
    } else {
        UsageSource::Estimated
    };
    (
        NormalizedUsage {
            input,
            output,
            ..Default::default()
        },
        source,
    )
}

fn local_count(state: &AppState, model: &str, map: Option<&Value>, body: &[u8]) -> u64 {
    #[cfg(feature = "count-local")]
    {
        crate::tokenize::count(model, body, map, &state.tokenizers)
    }
    #[cfg(not(feature = "count-local"))]
    {
        let _ = state;
        crate::tokenize::count(model, body, map, ())
    }
}

// ── upstream count endpoint (native only) ────────────────────────────────────

/// Global concurrency gate for settle-time upstream counts.
#[cfg(not(target_arch = "wasm32"))]
static COUNT_GATE: std::sync::OnceLock<tokio::sync::Semaphore> = std::sync::OnceLock::new();
#[cfg(not(target_arch = "wasm32"))]
const COUNT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Count input (original upstream request body) and output (produced text as
/// one user message) via the provider's count endpoint, through `channel.prepare`
/// + the same effective provider client — no pipeline, no user quota/authz. The
///   sealed secret is opened HERE and dropped on return.
#[cfg(not(target_arch = "wasm32"))]
async fn upstream_count(ctx: &SettleCtx, text: &str) -> Option<NormalizedUsage> {
    let gate = COUNT_GATE.get_or_init(|| tokio::sync::Semaphore::new(4));
    let _permit = gate.acquire().await.ok()?;
    let secret = ctx
        .state
        .cipher
        .open(&ctx.credential.secret_json)
        .map_err(|e| tracing::warn!(error = %e, "settle count: secret open failed"))
        .ok()?;
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
    Some(NormalizedUsage {
        input,
        output,
        ..Default::default()
    })
}

#[cfg(not(target_arch = "wasm32"))]
async fn count_once(ctx: &SettleCtx, secret: &Value, body: Bytes) -> Option<u64> {
    use crate::protocol::{Operation, OperationKey};
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
            upstream_model_id: &ctx.model,
            method: target.method.into(),
            path: &target.path,
            query: target.query.as_deref(),
            headers: &headers,
            body,
        })
        .ok()?;
    let client = ctx
        .state
        .upstream_client_for_credential(&ctx.channel, &ctx.credential, &ctx.provider)
        .map_err(|e| tracing::warn!(error = %e, "settle count: resolve upstream client failed"))
        .ok()?;
    let resp = tokio::time::timeout(COUNT_TIMEOUT, client.send(prepared.into_http()))
        .await
        .ok()?
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let v: Value = serde_json::from_slice(resp.body()).ok()?;
    // claude `input_tokens` / gemini `totalTokens` (openai never reaches here)
    v.get("input_tokens")
        .or_else(|| v.get("totalTokens"))
        .and_then(Value::as_u64)
}

/// Family-shaped count body for the PRODUCED text as one user message.
#[cfg(not(target_arch = "wasm32"))]
fn output_count_body(family: Family, model: &str, text: &str) -> Bytes {
    let v = match family {
        Family::Claude => json!({"model": model, "messages": [{"role": "user", "content": text}]}),
        Family::Gemini => json!({"contents": [{"role": "user", "parts": [{"text": text}]}]}),
        Family::OpenAi => json!({"model": model, "input": text}),
    };
    Bytes::from(serde_json::to_vec(&v).expect("json! serializes"))
}

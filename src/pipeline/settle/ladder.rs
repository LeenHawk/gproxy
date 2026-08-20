//! §17 counting ladder for abnormal/usage-less ends: gpt family → local
//! tiktoken; claude/gemini upstream family → upstream count endpoint (global
//! Semaphore(4) + 5s timeout, same effective provider client, no user quota/authz);
//! anything else / failure → local chain (vocab → chars/2).

use serde_json::{Value, json};

use super::SettleCtx;
use crate::app::AppState;
#[cfg(not(target_arch = "wasm32"))]
use crate::protocol::Provider as Family;
use crate::usage::{NormalizedUsage, UsageSource};

/// §17 counting ladder: gpt family → local tiktoken; claude/gemini upstream
/// family → upstream count endpoint (bounded concurrency + timeout, same
/// effective provider client, never the user pipeline); anything else / failure
/// → local chain (vocab → chars/2). The local chain is CPU-bound BPE encoding
/// of the full request + produced text (tens of ms for long chats), so on
/// native it runs on the blocking pool — never on an async worker whose other
/// streams it would stall.
pub(super) async fn ladder(ctx: &SettleCtx, text: String) -> (NormalizedUsage, UsageSource) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        if !crate::tokenize::is_gpt_family(&ctx.model)
            && matches!(ctx.upstream_family, Family::Claude | Family::Gemini)
        {
            match super::upstream_count::count(ctx, &text).await {
                Ok(usage) => return (usage, UsageSource::Counted),
                Err(reason) => tracing::warn!(
                    request_id = %ctx.request_id,
                    provider = %ctx.provider.name,
                    upstream_model = %ctx.model,
                    reason,
                    "upstream token count failed; using local estimate"
                ),
            }
        }
        let state = ctx.state.clone();
        let model = ctx.model.clone();
        let map = ctx.provider.settings_json.get("tokenizer_map").cloned();
        let request_body = ctx.request_body.clone();
        match tokio::task::spawn_blocking(move || {
            local_ladder(&state, &model, map.as_ref(), &request_body, &text)
        })
        .await
        {
            Ok(counted) => counted,
            Err(e) => {
                tracing::warn!(
                    request_id = %ctx.request_id,
                    provider = %ctx.provider.name,
                    upstream_model = %ctx.model,
                    error = %e,
                    "settle count task failed; recording zero estimate"
                );
                (NormalizedUsage::default(), UsageSource::Estimated)
            }
        }
    }
    #[cfg(target_arch = "wasm32")]
    local_ladder(
        &ctx.state,
        &ctx.model,
        ctx.provider.settings_json.get("tokenizer_map"),
        &ctx.request_body,
        &text,
    )
}

/// Local chain: input from the captured request body, output from the
/// produced text wrapped as a single user message. Pure CPU — callers on
/// native run this via `spawn_blocking`.
fn local_ladder(
    state: &AppState,
    model: &str,
    map: Option<&Value>,
    request_body: &[u8],
    text: &str,
) -> (NormalizedUsage, UsageSource) {
    let usage = local_estimate(state, model, map, request_body, text);
    // tiktoken is exact for gpt families; everything else is an estimate
    let source = if cfg!(feature = "count-local") && crate::tokenize::is_gpt_family(model) {
        UsageSource::Counted
    } else {
        UsageSource::Estimated
    };
    (usage, source)
}

/// Provider-operation fallback shared with embeddings, search, audio and
/// media settlement. The tokenizer facade is intentionally total: its last
/// rung is the chars/2 estimate.
pub(super) fn local_estimate(
    state: &AppState,
    model: &str,
    map: Option<&Value>,
    request_body: &[u8],
    produced_text: &str,
) -> NormalizedUsage {
    let input = local_count(state, model, map, request_body);
    let output = if produced_text.is_empty() {
        0
    } else {
        let body = serde_json::to_vec(&json!({
            "messages": [{ "role": "user", "content": produced_text }]
        }))
        .unwrap_or_default();
        local_count(state, model, map, &body)
    };
    let mut usage = NormalizedUsage {
        input,
        output,
        ..Default::default()
    };
    let input_text = crate::tokenize::harvest(request_body).0.join("\n");
    usage.set_metric(
        "input_characters",
        rust_decimal::Decimal::from(input_text.chars().count()),
    );
    usage
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

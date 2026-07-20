//! Claude Code request shaping and upstream request construction.

use bytes::Bytes;
use serde_json::Value;

use super::{auth, cch};
use crate::channel::http_util::{allow_headers, build_request, join_url};
use crate::channel::settings::RequestShapeSettings;
use crate::channel::shaping::{
    self, claude_cache_control, claude_fallback, claude_magic_cache, claude_sampling,
};
use crate::channel::{ChannelError, PrepareCtx, PreparedRequest, ShapeCtx};
use crate::protocol::{ContentGenerationKind, OperationKind};

fn is_claude_messages(op: crate::protocol::OperationKey) -> bool {
    matches!(
        op.kind,
        OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages)
    )
}

/// Claude Code model calls carry `beta=true`. Preserve any caller query and
/// avoid adding a duplicate beta key.
pub(super) fn model_query(query: Option<&str>) -> String {
    let Some(query) = query.map(str::trim).filter(|query| !query.is_empty()) else {
        return "beta=true".to_owned();
    };
    if query
        .split('&')
        .any(|pair| pair.split('=').next() == Some("beta"))
    {
        query.to_owned()
    } else {
        format!("beta=true&{query}")
    }
}

pub(super) fn shape(body: Bytes, headers: &mut http::HeaderMap, ctx: &ShapeCtx) -> Bytes {
    if !is_claude_messages(ctx.op) {
        return body;
    }
    let settings = RequestShapeSettings::from_value(ctx.settings);
    let body = shaping::with_json_body(body, |value| {
        claude_cache_control::sanitize_claude_body(value);
        claude_sampling::strip_sampling_params(value);
        if settings.enable_claude_fable_fallback {
            claude_fallback::apply_fable_to_opus48(value, headers);
        }
    });
    shaping::anthropic_beta::strip_beta_tokens(headers, &["context-1m-2025-08-07"]);
    body
}

pub(super) fn prepare(ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
    let access_token = auth::access_token(ctx.secret)?.to_string();
    let base = ctx
        .provider_settings
        .get("base_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(auth::DEFAULT_BASE_URL);

    // Stable per-credential `device_id`; an explicit downstream session id wins.
    // The same value is sent in the header and `metadata.user_id`.
    let device_id = auth::device_id(ctx.secret);
    let explicit_session_id = ctx
        .headers
        .get("x-claude-code-session-id")
        .or_else(|| ctx.headers.get("session_id"))
        .and_then(|value| value.to_str().ok());
    let session_id = cch::session_id(
        &device_id,
        explicit_session_id,
        crate::util::time::unix_now_ms(),
    );

    // Match the path exactly: `/v1/messages/count_tokens` rejects `metadata`.
    let is_messages = ctx.method == http::Method::POST && ctx.path == "/v1/messages";
    let body = if is_messages {
        let account_uuid = ctx
            .secret
            .get("account_uuid")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let settings = RequestShapeSettings::from_value(ctx.provider_settings);
        Bytes::from(cch::apply(
            &claude_magic_cache::apply_if_enabled(ctx.body, settings.enable_magic_cache),
            &device_id,
            account_uuid,
            &session_id,
        ))
    } else {
        ctx.body
    };

    let model_query = is_messages.then(|| model_query(ctx.query));
    let uri = join_url(base, ctx.path, model_query.as_deref().or(ctx.query))?;
    let headers = allow_headers(ctx.headers, &["anthropic-beta"]);
    let mut request = build_request(ctx.method, uri, headers, body)?;
    auth::apply(&mut request, &access_token, &session_id)?;
    Ok(PreparedRequest::new(request))
}

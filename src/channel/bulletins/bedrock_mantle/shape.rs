use bytes::Bytes;
use http::HeaderMap;

use crate::channel::ShapeCtx;
use crate::channel::settings::RequestShapeSettings;
use crate::channel::shaping::{
    self, claude_cache_control, claude_fallback, claude_magic_cache, openai_cache,
};
use crate::protocol::{ContentGenerationKind, OperationKind};

pub(super) fn request(body: Bytes, headers: &mut HeaderMap, ctx: &ShapeCtx) -> Bytes {
    let settings = RequestShapeSettings::from_value(ctx.settings);
    if let Some(kind) = openai_cache::kind_for_operation(ctx.op) {
        if !settings.enable_magic_cache {
            return body;
        }
        return shaping::with_json_body(body, |value| {
            openai_cache::apply_magic_string_cache_breakpoints(value, kind)
        });
    }
    if !is_claude_messages(ctx)
        || (!settings.enable_magic_cache && !settings.enable_claude_fable_fallback)
    {
        return body;
    }
    shaping::with_json_body(body, |value| {
        if settings.enable_magic_cache {
            claude_magic_cache::apply_magic_string_cache_control_triggers(value);
            claude_cache_control::sanitize_claude_body(value);
        }
        if settings.enable_claude_fable_fallback {
            claude_fallback::apply_fable_to_opus48(value, headers);
        }
    })
}

fn is_claude_messages(ctx: &ShapeCtx) -> bool {
    ctx.op.kind == OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages)
}

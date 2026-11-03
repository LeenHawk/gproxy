//! Request/response helpers shared by the OpenAI-compatible channels.

use crate::channel::ChannelError;

#[allow(dead_code)] // not every compatible channel reports response errors
pub(crate) fn invalid_response(message: impl Into<String>) -> ChannelError {
    ChannelError::InvalidResponse(message.into())
}

pub(crate) use crate::channel::read_body;

pub(crate) use crate::channel::insert_configured;
#[cfg(any(
    feature = "dashscope",
    feature = "cline",
    feature = "commandcode",
    feature = "vercel",
    feature = "volcengine",
    feature = "minimax",
    feature = "deepseek",
    feature = "kimi",
    feature = "nvidia",
    feature = "opencode",
    feature = "cloudflare_ai_gateway",
    feature = "copilotcli",
    feature = "openrouter",
    feature = "xai",
    feature = "glm",
    feature = "grokbuild",
))]
pub(crate) use crate::channel::strip_query_auth;

use gproxy_channel_api::{ChannelError, ChannelTrafficPolicy, PrepareCtx};

pub(crate) fn request_headers(
    policy: ChannelTrafficPolicy,
    ctx: &PrepareCtx<'_>,
) -> Result<http::HeaderMap, ChannelError> {
    policy
        .filter_request_headers(ctx.headers, ctx.provider_settings)
        .map_err(ChannelError::Prepare)
}

pub(crate) fn request_query(
    policy: ChannelTrafficPolicy,
    ctx: &PrepareCtx<'_>,
) -> Result<Option<String>, ChannelError> {
    policy
        .filter_request_query(ctx.query, ctx.provider_settings)
        .map_err(ChannelError::Prepare)
}

const NONE: &[&str] = &[];
const ANTHROPIC: &[&str] = &["anthropic-beta"];
const OPENAI: &[&str] = &["openai-beta", "openai-organization", "openai-project"];
const OPENAI_QUERY: &[&str] = &["after", "limit", "order", "purpose", "variant"];
const CLAUDE_QUERY: &[&str] = &["after_id", "before_id", "limit"];
const GEMINI_QUERY: &[&str] = &["alt", "pageSize", "pageToken"];
const OPENAI_RESPONSE: &[&str] = &["openai-*", "x-ratelimit-*", "x-request-id"];
const CLAUDE_RESPONSE: &[&str] = &["anthropic-*", "request-id", "x-ratelimit-*"];
const GEMINI_RESPONSE: &[&str] = &["x-gemini-*", "x-goog-*", "x-request-id"];

pub(crate) const AISTUDIO: ChannelTrafficPolicy = ChannelTrafficPolicy::new(
    &[
        "range",
        "x-goog-upload-command",
        "x-goog-upload-header-content-length",
        "x-goog-upload-header-content-type",
        "x-goog-upload-offset",
        "x-goog-upload-protocol",
    ],
    GEMINI_RESPONSE,
    GEMINI_QUERY,
);
pub(crate) const ANTIGRAVITY: ChannelTrafficPolicy =
    ChannelTrafficPolicy::new(NONE, GEMINI_RESPONSE, NONE);
pub(crate) const AWS_BEDROCK: ChannelTrafficPolicy = ChannelTrafficPolicy::new(
    &["anthropic-beta", "openai-beta"],
    &["x-amz-*", "x-amzn-*"],
    &[
        "byCustomizationType",
        "byInferenceType",
        "byOutputModality",
        "byProvider",
    ],
);
pub(crate) const AZURE: ChannelTrafficPolicy = ChannelTrafficPolicy::new(
    &["anthropic-beta", "openai-beta"],
    &["apim-request-id", "x-ms-*", "x-ratelimit-*", "x-request-id"],
    &[
        "after",
        "api-version",
        "azure-beta",
        "limit",
        "order",
        "variant",
    ],
);
pub(crate) const CLAUDE_API: ChannelTrafficPolicy = ChannelTrafficPolicy::new(
    &["anthropic-beta", "anthropic-user-profile-id"],
    CLAUDE_RESPONSE,
    CLAUDE_QUERY,
);
pub(crate) const CLAUDE_CODE: ChannelTrafficPolicy =
    ChannelTrafficPolicy::new(ANTHROPIC, CLAUDE_RESPONSE, &["*"]);
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const CLAUDE_WEB: ChannelTrafficPolicy =
    ChannelTrafficPolicy::new(NONE, CLAUDE_RESPONSE, NONE);
pub(crate) const CLINE: ChannelTrafficPolicy =
    ChannelTrafficPolicy::new(NONE, &["x-request-id"], NONE);
pub(crate) const CLOUDFLARE: ChannelTrafficPolicy = ChannelTrafficPolicy::new(
    &[
        "cf-aig-skip-cache",
        "cf-aig-cache-ttl",
        "cf-aig-cache-key",
        "cf-aig-collect-log",
        "cf-aig-request-timeout",
        "cf-aig-max-attempts",
        "cf-aig-retry-delay",
        "cf-aig-backoff",
        "cf-aig-metadata",
    ],
    &["cf-*", "x-request-id"],
    NONE,
);
pub(crate) const CODEX: ChannelTrafficPolicy = ChannelTrafficPolicy::new(
    &[
        "if-none-match",
        "oai-product-sku",
        "openai-alpha",
        "openai-beta",
        "originator",
        "session-id",
        "thread-id",
        "user-agent",
        "version",
        "x-client-request-id",
        "x-codex-*",
        "x-openai-memgen-request",
        "x-openai-subagent",
        "x-openai-internal-codex-residency",
        "x-session-id",
    ],
    &[
        "etag",
        "mcp-session-id",
        "openai-*",
        "x-codex-*",
        "x-ratelimit-*",
        "x-request-id",
    ],
    &["*"],
);
pub(crate) const COPILOT: ChannelTrafficPolicy =
    ChannelTrafficPolicy::new(NONE, &["x-github-*", "x-ratelimit-*", "x-request-id"], NONE);
pub(crate) const CUSTOM: ChannelTrafficPolicy = ChannelTrafficPolicy::new(
    &["anthropic-beta", "openai-beta", "range"],
    &[
        "anthropic-*",
        "openai-*",
        "x-goog-*",
        "x-ratelimit-*",
        "x-request-id",
    ],
    &[
        "after",
        "after_id",
        "alt",
        "before_id",
        "limit",
        "order",
        "pageSize",
        "pageToken",
        "purpose",
        "variant",
    ],
);
pub(crate) const DASHSCOPE: ChannelTrafficPolicy = ChannelTrafficPolicy::new(
    NONE,
    &["x-dashscope-*", "x-ratelimit-*", "x-request-id"],
    NONE,
);
pub(crate) const DEEPSEEK: ChannelTrafficPolicy =
    ChannelTrafficPolicy::new(NONE, OPENAI_RESPONSE, NONE);
pub(crate) const GEMINI_CLI: ChannelTrafficPolicy =
    ChannelTrafficPolicy::new(NONE, GEMINI_RESPONSE, NONE);
pub(crate) const GROK_BUILD: ChannelTrafficPolicy =
    ChannelTrafficPolicy::new(NONE, &["x-grok-*", "x-ratelimit-*", "x-request-id"], &["*"]);
pub(crate) const OPENAI_COMPATIBLE: ChannelTrafficPolicy =
    ChannelTrafficPolicy::new(NONE, OPENAI_RESPONSE, NONE);
pub(crate) const KIMI: ChannelTrafficPolicy =
    ChannelTrafficPolicy::new(ANTHROPIC, OPENAI_RESPONSE, &["after", "limit"]);
pub(crate) const KIRO: ChannelTrafficPolicy =
    ChannelTrafficPolicy::new(NONE, &["x-amz-*", "x-amzn-*"], NONE);
pub(crate) const OPENAI_API: ChannelTrafficPolicy =
    ChannelTrafficPolicy::new(OPENAI, OPENAI_RESPONSE, OPENAI_QUERY);
pub(crate) const OPENROUTER: ChannelTrafficPolicy = ChannelTrafficPolicy::new(
    &["http-referer", "x-title"],
    &["x-openrouter-*", "x-ratelimit-*", "x-request-id"],
    &[
        "category",
        "index",
        "output_modalities",
        "supported_parameters",
        "use_rss",
        "use_rss_chat_links",
    ],
);
pub(crate) const VERCEL: ChannelTrafficPolicy = ChannelTrafficPolicy::new(
    ANTHROPIC,
    &["x-ratelimit-*", "x-request-id", "x-vercel-*"],
    NONE,
);
pub(crate) const VERTEX: ChannelTrafficPolicy =
    ChannelTrafficPolicy::new(ANTHROPIC, GEMINI_RESPONSE, GEMINI_QUERY);
pub(crate) const VERTEX_EXPRESS: ChannelTrafficPolicy =
    ChannelTrafficPolicy::new(NONE, GEMINI_RESPONSE, GEMINI_QUERY);
pub(crate) const WORKBUDDY: ChannelTrafficPolicy =
    ChannelTrafficPolicy::new(NONE, &["x-request-id"], NONE);
pub(crate) const XAI: ChannelTrafficPolicy = ChannelTrafficPolicy::new(
    &["x-grok-conv-id"],
    &["x-grok-*", "x-ratelimit-*", "x-request-id"],
    NONE,
);

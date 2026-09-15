//! Built-in billable quantity keys. PriceRate.metric also accepts custom keys.
//! These definitions do not implement usage extraction or settlement.
//!
//! Rates must consume disjoint billable quantities: cached input is removed
//! from ordinary input; reasoning/media token subsets must not be charged again
//! when already billed through aggregate input/output. Per-image and per-token
//! output are alternative billing bases unless the provider charges both.

// Text generation and embedding: token units, conventionally per 1_000_000.
pub const INPUT_TOKENS: &str = "input_tokens";
pub const OUTPUT_TOKENS: &str = "output_tokens";
pub const CACHED_INPUT_TOKENS: &str = "cached_input_tokens";
pub const CACHE_CREATION_5M_TOKENS: &str = "cache_creation_5m_tokens";
pub const CACHE_CREATION_30M_TOKENS: &str = "cache_creation_30m_tokens";
pub const CACHE_CREATION_1H_TOKENS: &str = "cache_creation_1h_tokens";
pub const REASONING_TOKENS: &str = "reasoning_tokens";

// Images: separate token billing from per-image billing.
pub const IMAGE_INPUT_TOKENS: &str = "image_input_tokens";
pub const IMAGE_OUTPUT_TOKENS: &str = "image_output_tokens";
/// Count; rate conditions can select size, quality and other reported dimensions.
pub const IMAGE_OUTPUTS: &str = "image_outputs";

// Audio: token, duration or character billing according to the upstream product.
pub const AUDIO_INPUT_TOKENS: &str = "audio_input_tokens";
pub const CACHED_AUDIO_INPUT_TOKENS: &str = "cached_audio_input_tokens";
pub const AUDIO_OUTPUT_TOKENS: &str = "audio_output_tokens";
/// Seconds; use a denominator of 60 for a per-minute price.
pub const AUDIO_SECONDS: &str = "audio_seconds";
/// Characters for speech synthesis priced by text length.
pub const AUDIO_CHARACTERS: &str = "audio_characters";

// Video: token, duration or per-video billing.
pub const VIDEO_INPUT_TOKENS: &str = "video_input_tokens";
/// Retains the v3 metric key for video token billing.
pub const VIDEO_TOKENS: &str = "video_tokens";
pub const VIDEO_SECONDS: &str = "video_seconds";
pub const VIDEO_OUTPUTS: &str = "video_outputs";

// Rerank and server-side tools: count units, per call/session/search unit.
pub const SEARCH_UNITS: &str = "search_units";
pub const WEB_SEARCHES: &str = "web_searches";
pub const WEB_FETCHES: &str = "web_fetches";
pub const FILE_SEARCHES: &str = "file_searches";
pub const CODE_INTERPRETER_SESSIONS: &str = "code_interpreter_sessions";
/// Generic billable tool calls; conditions may select tool_name. Do not also
/// bill a specific tool counter for the same call. Client tool declarations
/// alone do not establish a billable server-side execution.
pub const TOOL_CALLS: &str = "tool_calls";
pub const REQUESTS: &str = "requests";

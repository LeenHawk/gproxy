use gproxy_channel_api::{ChannelField, ChannelFieldControl::*};

const fn field(
    key: &'static str,
    control: gproxy_channel_api::ChannelFieldControl,
    required: bool,
    advanced: bool,
) -> ChannelField {
    ChannelField {
        key,
        control,
        required,
        advanced,
        default_value: None,
    }
}

pub(crate) const BASE_URL: &[ChannelField] = &[field("base_url", Url, false, false)];
pub(crate) const CUSTOM: &[ChannelField] = &[
    field("base_url", Url, false, false),
    field("enable_openai_magic_cache", Boolean, false, true),
    field("claude_fable_fallbacks", StringList, false, true),
];
pub(crate) const BEDROCK: &[ChannelField] = &[
    field("base_url", Url, false, false),
    field("region", Text, false, false),
    field("video_output_s3_uri", Text, false, true),
];
pub(crate) const VERTEX: &[ChannelField] = &[
    field("base_url", Url, false, false),
    field("location", Text, false, false),
    field("oauth_client_id", Text, false, true),
    field("oauth_client_secret", Secret, false, true),
    field("oauth_token_url", Url, false, true),
];
pub(crate) const KIRO: &[ChannelField] = &[
    field("base_url", Url, false, false),
    field("region", Text, false, false),
    field("profile_arn", Text, false, true),
    field("auth_base_url", Url, false, true),
];
pub(crate) const OPENCODE: &[ChannelField] = &[
    field("base_url", Url, false, false),
    field("tier", Text, false, false),
    field("console_base_url", Url, false, true),
];
pub(crate) const OAUTH: &[ChannelField] = &[
    field("access_token", Secret, true, false),
    field("refresh_token", Secret, false, true),
];
pub(crate) const API_KEY: &[ChannelField] = &[field("api_key", Secret, true, false)];
pub(crate) const API_KEY_OR_OAUTH: &[ChannelField] = &[
    field("api_key", Secret, false, false),
    field("access_token", Secret, false, false),
    field("refresh_token", Secret, false, true),
];
pub(crate) const SERVICE_ACCOUNT: &[ChannelField] = &[
    field("client_email", Text, true, false),
    field("private_key", Secret, true, false),
    field("project_id", Text, true, false),
    field("access_token", Secret, false, true),
];
pub(crate) const GOOGLE_OAUTH: &[ChannelField] = &[
    field("access_token", Secret, true, false),
    field("refresh_token", Secret, false, true),
    field("project_id", Text, true, false),
];
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const CLAUDE_WEB: &[ChannelField] = &[
    field("cookie", Secret, true, false),
    field("account_uuid", Text, true, false),
];
pub(crate) const GITHUB: &[ChannelField] = &[field("github_token", Secret, true, false)];
pub(crate) const AWS: &[ChannelField] = &[
    field("api_key", Secret, false, false),
    field("access_key_id", Text, false, false),
    field("secret_access_key", Secret, false, false),
    field("session_token", Secret, false, true),
];

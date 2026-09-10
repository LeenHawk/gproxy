use gproxy_channel_api::{ChannelField, ChannelFieldControl};

const fn field(key: &'static str, control: ChannelFieldControl) -> ChannelField {
    ChannelField {
        key,
        i18n_key: key,
        control,
        required: false,
        advanced: true,
        default_value: None,
        options: &[],
    }
}

const MANAGEMENT_KEY: &[ChannelField] = &[
    field("quota_api_key", ChannelFieldControl::Secret),
    field("quota_base_url", ChannelFieldControl::Url),
];
const XAI: &[ChannelField] = &[
    field("quota_api_key", ChannelFieldControl::Secret),
    field("quota_team_id", ChannelFieldControl::Text),
    field("quota_base_url", ChannelFieldControl::Url),
];

const OPENCODE: &[ChannelField] = &[
    field("quota_cookie", ChannelFieldControl::Secret),
    field("quota_workspace_id", ChannelFieldControl::Text),
    field("quota_base_url", ChannelFieldControl::Url),
];

pub(crate) fn fields(channel: &str) -> Option<&'static [ChannelField]> {
    match channel {
        "openrouter" | "openai" | "claudeapi" => Some(MANAGEMENT_KEY),
        "xai" => Some(XAI),
        "opencode" => Some(OPENCODE),
        _ => None,
    }
}

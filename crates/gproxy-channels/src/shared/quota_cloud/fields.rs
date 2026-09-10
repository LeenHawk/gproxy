use gproxy_channel_api::{ChannelField, ChannelFieldControl, ChannelFieldControl::*};

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

const AWS: &[ChannelField] = &[
    field("quota_access_key_id", Secret),
    field("quota_access_key_secret", Secret),
    field("quota_session_token", Secret),
    field("quota_region", Text),
    field("quota_quota_code", Text),
    field("quota_base_url", Url),
];
const GOOGLE: &[ChannelField] = &[
    field("quota_api_key", Secret),
    field("quota_project_id", Text),
    field("quota_base_url", Url),
];
const AZURE: &[ChannelField] = &[
    field("quota_api_key", Secret),
    field("quota_subscription_id", Text),
    field("quota_region", Text),
    field("quota_base_url", Url),
];
const ALIYUN: &[ChannelField] = &[
    field("quota_access_key_id", Secret),
    field("quota_access_key_secret", Secret),
    field("quota_session_token", Secret),
    field("quota_base_url", Url),
];
const CLOUDFLARE: &[ChannelField] = &[
    field("quota_api_key", Secret),
    field("quota_account_id", Text),
    field("quota_base_url", Url),
];

pub(crate) fn fields(channel: &str) -> Option<&'static [ChannelField]> {
    Some(match channel {
        "aws-bedrock" => AWS,
        "vertex" | "aistudio" | "vertexexpress" => GOOGLE,
        "azure" => AZURE,
        "dashscope" => ALIYUN,
        "cloudflare-ai-gateway" => CLOUDFLARE,
        _ => return None,
    })
}

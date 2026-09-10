use gproxy_channel_api::{QuotaKind, QuotaQueryMode, QuotaSource, QuotaSupport};
use serde_json::Value;

pub(crate) fn ready(id: &str, label: &str, kind: QuotaKind) -> QuotaSource {
    QuotaSource {
        id: id.into(),
        label: label.into(),
        kinds: vec![kind],
        mode: QuotaQueryMode::Probe,
        support: QuotaSupport::Ready,
        reason: None,
        automatic: true,
    }
}

fn unavailable(id: &str, label: &str, kind: QuotaKind, auth: bool, reason: &str) -> QuotaSource {
    QuotaSource {
        id: id.into(),
        label: label.into(),
        kinds: vec![kind],
        mode: QuotaQueryMode::Unavailable,
        support: if auth {
            QuotaSupport::RequiresAuthorization
        } else {
            QuotaSupport::Unsupported
        },
        reason: Some(reason.into()),
        automatic: false,
    }
}

pub(crate) fn sources(channel: &str, secret: &Value, settings: &Value) -> Vec<QuotaSource> {
    if let Some(sources) = super::quota_cloud::sources(channel, secret, settings)
        .or_else(|| super::quota_management::sources(channel, secret, settings))
    {
        return sources;
    }
    use QuotaKind::{Balance, UsageReport};
    let source = match channel {
        "deepseek" => ready("balance", "Account balance", Balance),
        "vercel" => {
            return vec![
                ready("balance", "Team balance", Balance),
                unavailable(
                    "usage_report",
                    "Team spend reports",
                    UsageReport,
                    false,
                    "Vercel custom reports cost $0.005 per query; paid reporting is not integrated. Account balance remains available.",
                ),
            ];
        }
        "custom" if siliconflow(settings) => ready(
            "siliconflow_balance",
            "SiliconFlow account balance",
            Balance,
        ),
        "nvidia" => unavailable(
            "quota",
            "NVIDIA quota",
            Balance,
            false,
            "API Catalog and self-hosted NIM have different billing models; no common independent balance query is confirmed.",
        ),
        _ => unavailable(
            "quota",
            "Upstream quota",
            Balance,
            false,
            "No supported upstream quota query is available for this channel. OpenAI-compatible inference does not imply compatible billing endpoints.",
        ),
    };
    vec![source]
}

pub(crate) fn siliconflow(settings: &Value) -> bool {
    settings
        .get("base_url")
        .and_then(Value::as_str)
        .and_then(|base| base.trim().parse::<http::Uri>().ok())
        .is_some_and(|uri| {
            uri.scheme_str() == Some("https")
                && uri.host() == Some("api.siliconflow.cn")
                && matches!(uri.path().trim_end_matches('/'), "" | "/v1")
                && uri.query().is_none()
                && uri
                    .authority()
                    .is_some_and(|authority| !authority.as_str().contains('@'))
        })
}

pub(crate) fn fields(channel: &str) -> &'static [gproxy_channel_api::ChannelField] {
    super::quota_cloud::fields(channel)
        .or_else(|| super::quota_management::fields(channel))
        .unwrap_or(&[])
}

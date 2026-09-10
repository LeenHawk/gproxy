use super::super::{quota_api::field, quota_catalog::ready};
use gproxy_channel_api::{QuotaKind, QuotaQueryMode, QuotaSource, QuotaSupport};
use serde_json::Value;

pub(crate) fn sources(channel: &str, secret: &Value, settings: &Value) -> Option<Vec<QuotaSource>> {
    let key = field(secret, "quota_api_key").is_some();
    match channel {
        "openrouter" => Some(vec![
            ready("key", "Key budget", QuotaKind::Budget),
            managed(
                "account_balance",
                "Account credits",
                QuotaKind::Balance,
                key,
                true,
                "Account credits require quota_api_key containing an OpenRouter Management key; inference key budget remains independent.",
            ),
        ]),
        "openai" => Some(vec![
            response(),
            managed(
                "organization_usage",
                "Organization daily costs (last 7 complete UTC days)",
                QuotaKind::UsageReport,
                key,
                false,
                "Organization Costs requires quota_api_key containing an OpenAI Admin key. Manual query; this is historical spending, not prepaid balance.",
            ),
        ]),
        "claudeapi" => {
            let mut report = ready(
                "organization_usage",
                "Organization daily costs (last 7 complete UTC days)",
                QuotaKind::UsageReport,
            );
            report.automatic = false;
            report.reason = Some("Manual query with quota_api_key when supplied, otherwise the existing inference key. Requires an Admin key or an eligible non-workspace personal/service key. Priority Tier costs are excluded upstream.".into());
            Some(vec![response(), report])
        }
        "xai" => {
            let team = field(secret, "quota_team_id")
                .or_else(|| field(settings, "quota_team_id"))
                .is_some();
            let configured = key && team;
            Some(vec![
                managed(
                    "prepaid_balance",
                    "Team prepaid balance",
                    QuotaKind::Balance,
                    configured,
                    true,
                    "Requires quota_api_key containing an xAI Management key and quota_team_id with billing-read permission. The signed ledger total is normalized to remaining USD: negative ledger credit becomes positive remaining balance. Postpaid availability is not inferred from prepaid balance.",
                ),
                managed(
                    "postpaid_budget",
                    "Team monthly postpaid budget",
                    QuotaKind::Budget,
                    configured,
                    true,
                    "Requires quota_api_key containing an xAI Management key and quota_team_id with billing-read permission. Effective monthly postpaid limit is independent of prepaid credit; used and remaining are not inferred.",
                ),
            ])
        }
        "opencode" => {
            let mut sources = Vec::new();
            if super::opencode::is_go(settings) {
                let mut source = ready(
                    "go_subscription",
                    "OpenCode Go user/workspace quota",
                    QuotaKind::Window,
                );
                source.reason = Some("Uses the existing API key with the official Go usage endpoint. A Go subscription is required; Console wallet balance is independent.".into());
                sources.push(source);
            }
            let mut console = managed(
                "console_balance",
                "OpenCode Console wallet and monthly budget",
                QuotaKind::Balance,
                field(secret, "quota_cookie").is_some()
                    && field(secret, "quota_workspace_id").is_some(),
                true,
                "Requires the OpenCode Console auth cookie and workspace ID. Reads the authenticated billing page; this wallet is separate from Go subscription quota. Expired sessions retain the previous observation.",
            );
            console.kinds.push(QuotaKind::Budget);
            sources.push(console);
            Some(sources)
        }
        _ => None,
    }
}

fn response() -> QuotaSource {
    let mut source = ready(
        "response_limits",
        "Rate limits from responses",
        QuotaKind::RateLimit,
    );
    source.mode = QuotaQueryMode::Response;
    source.automatic = false;
    source
}

fn managed(
    id: &str,
    label: &str,
    kind: QuotaKind,
    configured: bool,
    automatic: bool,
    reason: &str,
) -> QuotaSource {
    let mut source = ready(id, label, kind);
    source.automatic = automatic && configured;
    source.reason = Some(reason.into());
    if !configured {
        source.support = QuotaSupport::RequiresAuthorization;
        source.mode = QuotaQueryMode::Unavailable;
    }
    source
}

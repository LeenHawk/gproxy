//! Neutral answers for accounting the gateway cannot reconstruct. The raw
//! credential view is handled before this module: its replies pass through.
//! Caller/Pool must not turn unknown historical allowances or vendor credits
//! into zeros, nor expose another user's tasks or plugin invocations.
use crate::channel::ChannelError;
use crate::channels::shared::services_common::{json_response, read_body};
use gproxy_protocol::wire::openai::codex::{analytics::*, usage::*};
use gproxy_protocol::{HttpBody, Rest, WireRequest, WireResponse};
use http::StatusCode;
use serde::Serialize;
use serde_json::json;

fn unavailable() -> Rest {
    serde_json::Map::from_iter([
        ("data_status".into(), json!("unavailable")),
        ("usage_source".into(), json!("gproxy")),
    ])
}

fn response(value: impl Serialize) -> Result<Option<WireResponse>, ChannelError> {
    let value =
        serde_json::to_value(value).map_err(|e| ChannelError::InvalidResponse(e.to_string()))?;
    Ok(Some(json_response(StatusCode::OK, &value)))
}

pub(super) async fn answer(
    path: &str,
    request: WireRequest<HttpBody>,
) -> Result<Option<WireResponse>, ChannelError> {
    let path = path.strip_prefix("/backend-api/wham/").unwrap_or(path);
    let query = |key: &str| {
        request.query.as_deref().and_then(|query| {
            url::form_urlencoded::parse(query.as_bytes())
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.into_owned())
        })
    };
    match path {
        "usage/thread_usage/query_v2" => {
            let bytes = read_body(request.body).await?;
            let Ok(body) = serde_json::from_slice::<TaskUsageRequestBody>(&bytes) else {
                return Ok(Some(json_response(
                    StatusCode::BAD_REQUEST,
                    &json!({"detail":"invalid task usage query"}),
                )));
            };
            let mut ids = std::collections::HashSet::new();
            if body.threads.is_empty()
                || body.threads.len() > 100
                || body.threads.iter().any(|thread| {
                    std::iter::once(&thread.thread_id)
                        .chain(&thread.descendant_thread_ids)
                        .any(|id| id.trim().is_empty() || id.len() > 512 || !ids.insert(id))
                })
                || ids.len() > 1000
            {
                return Ok(Some(json_response(
                    StatusCode::BAD_REQUEST,
                    &json!({"detail":"expected at most 100 disjoint tasks and 1000 thread IDs"}),
                )));
            }
            response(TaskUsageResponseBody {
                data_as_of: None,
                threads: body
                    .threads
                    .into_iter()
                    .map(|thread| TaskUsage {
                        thread_id: thread.thread_id,
                        data_status: TaskUsageStatus::Unavailable,
                        usage_source: "gproxy".into(),
                        five_hour_limit_percent: None,
                        weekly_limit_percent: None,
                        balance_usage_credits: None,
                        groups: vec![],
                        rest: Rest::new(),
                    })
                    .collect(),
                rest: Rest::new(),
            })
        }
        "usage/plan_limit_history" => response(PlanLimitHistory {
            data_as_of: None,
            coverage_start: None,
            coverage_complete: false,
            approximate: true,
            boundary_tolerance_seconds: None,
            periods: vec![],
            rest: unavailable(),
        }),
        "usage/daily-token-usage-breakdown"
        | "usage/daily-workspace-user-token-usage-breakdown" => {
            response(DailyProductSurfaceUsageResponse {
                data: vec![],
                units: None,
                data_freshness_ts: None,
                group_by: query("group_by"),
                breakdown_by: None,
                rest: unavailable(),
            })
        }
        "usage/credit-usage-events" => response(CreditUsageEventsResponse {
            data: vec![],
            rest: unavailable(),
        }),
        "usage/daily-workspace-user-credit-usage" => response(CurrentUserCreditUsageResponse {
            breakdown: query("breakdown").unwrap_or_default(),
            data: vec![],
            series: vec![],
            unit: None,
            data_freshness_ts: None,
            rest: unavailable(),
        }),
        "analytics/daily-workspace-usage-counts" => response(DailyWorkspaceUsageCountResponse {
            data: vec![],
            balance_unit: None,
            active_users_summary: None,
            group_by: query("group_by"),
            breakdown_by: None,
            rest: unavailable(),
        }),
        "analytics/daily-plugin-usage-metrics" => response(PluginUsageMetricsResponse {
            data: vec![],
            data_freshness_ts: None,
            group_by: query("group_by"),
            rest: unavailable(),
        }),
        "analytics/daily-skill-usage-metrics" => response(DailySkillUsageMetricsResponse {
            data: vec![],
            data_freshness_ts: None,
            group_by: query("group_by"),
            rest: unavailable(),
        }),
        _ => Ok(None),
    }
}

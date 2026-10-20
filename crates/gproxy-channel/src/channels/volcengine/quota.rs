//! get-afp-usage-api / get-seat-afp-usage-api / get-seat-info-usage-api.
//! AFP amounts and window timestamps come from the API, never plan-price tables.
//! Coding personal: official ArkCLI `usage plan --product coding-plan`:
//! https://github.com/volcengine/ark-cli/blob/dd4c0dc2990da62be9fc040f431050aa2b2d508c/skills/arkcli-usage/references/arkcli-usage-plan.md
//! GetCodingPlanUsage reports percentages, not absolute quotas; its reset
//! timestamps are seconds, unlike the AFP API's millisecond timestamps.
//! Current console also reports Cap (default 100) for bonus quota:
//! https://res.gcloudcache.com/volc-fe/console-ark/model-settings/static/js/async/8010.5b5d85a508.js

use super::{Config, Edition, Volcengine, control};
use crate::channel::{
    ChannelError, CredentialContext, CredentialView, OperationFuture, ProviderView, QuotaAllowance,
    QuotaDimension, QuotaEntry, QuotaMetric, QuotaModel, QuotaQuery, QuotaResetBehavior,
    QuotaScope, QuotaSnapshot, QuotaSubject, QuotaTracking, QuotaValue, QuotaWindow,
};
use crate::channels::shared::compatible::{ability::decimal, http::invalid_response};
use gproxy_protocol::Operation;
use serde_json::{Value, json};

const WINDOWS: [(&str, &str); 4] = [
    ("AFPFiveHour", "5 hours"),
    ("AFPDaily", "daily"),
    ("AFPWeekly", "weekly"),
    ("AFPMonthly", "monthly"),
];

const CODING_WINDOWS: [(&str, &str); 3] = [
    ("session", "5 hours"),
    ("weekly", "weekly"),
    ("monthly", "monthly"),
];

async fn coding_personal(ctx: CredentialContext<'_>) -> Result<QuotaSnapshot, ChannelError> {
    let result = control::call(ctx, "GetCodingPlanUsage", json!({}), None).await?;
    let windows = result
        .get("QuotaUsage")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_response("Ark Coding Plan reply missing QuotaUsage"))?;
    // An empty list means this identity has no active Coding Plan subscription.
    let entries = windows
        .iter()
        .filter_map(|window| {
            let level = window.get("Level").and_then(Value::as_str)?;
            let (_, label) = CODING_WINDOWS.iter().find(|(name, _)| *name == level)?;
            let used = window.get("Percent").and_then(decimal)?;
            let limit = window.get("Cap").and_then(decimal).unwrap_or(100.into());
            Some(QuotaEntry {
                id: format!("coding_{level}"),
                source_id: format!("coding_{level}"),
                label: Some(format!("Coding Plan ({label})")),
                subject: QuotaSubject::Account,
                model_scope: QuotaScope::All,
                value: QuotaValue::Window(QuotaAllowance {
                    used: Some(used),
                    limit: Some(limit),
                    remaining: Some((limit - used).max(0.into())),
                    used_percent: (limit > 0.into())
                        .then(|| used / limit * rust_decimal::Decimal::from(100)),
                    unit: Some("percent".into()),
                    period_end_ms: window
                        .get("ResetTimestamp")
                        .and_then(Value::as_i64)
                        .filter(|seconds| *seconds > 0)
                        .and_then(|seconds| seconds.checked_mul(1000)),
                    reset_behavior: QuotaResetBehavior::Periodic,
                    ..Default::default()
                }),
            })
        })
        .collect();
    Ok(QuotaSnapshot {
        observed_at_ms: 0,
        entries,
    })
}

impl QuotaQuery for Volcengine {
    fn query<'a>(&'a self, ctx: CredentialContext<'a>) -> OperationFuture<'a, QuotaSnapshot> {
        Box::pin(async move {
            let config = Config::read(ctx.provider)?;
            let enterprise = matches!(config.edition, Edition::Enterprise);
            if !self.agent && !enterprise {
                return coding_personal(ctx).await;
            }
            let seat = if enterprise {
                Some(
                    ctx.credential
                        .metadata
                        .get("seat_id")
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                        .ok_or_else(|| {
                            ChannelError::InvalidConfig(
                                "Enterprise Ark quota requires credential metadata.seat_id".into(),
                            )
                        })?,
                )
            } else {
                None
            };
            let result = if self.agent {
                match seat {
                    Some(seat) => {
                        let result =
                            control::call(ctx, "GetSeatAFPUsage", json!({"SeatIDs": [seat]}), None)
                                .await?;
                        result
                            .get("SeatAFPUsages")
                            .and_then(Value::as_array)
                            .and_then(|rows| {
                                rows.iter().find(|row| row["SeatID"].as_str() == Some(seat))
                            })
                            .cloned()
                            .ok_or_else(|| {
                                invalid_response("Ark quota reply missing requested seat")
                            })?
                    }
                    None => control::call(ctx, "GetAFPUsage", json!({}), None).await?,
                }
            } else {
                let seat = seat.ok_or_else(|| {
                    ChannelError::InvalidConfig(
                        "Enterprise Ark quota requires credential metadata.seat_id".into(),
                    )
                })?;
                let mut body = json!({"SeatID": seat});
                if let Some(project) = ctx
                    .credential
                    .metadata
                    .get("project_name")
                    .and_then(Value::as_str)
                {
                    body["ProjectName"] = project.into();
                }
                control::call(ctx, "GetSeatInfoUsage", body, None)
                    .await?
                    .get("SeatInfoUsage")
                    .cloned()
                    .ok_or_else(|| invalid_response("Ark quota reply missing SeatInfoUsage"))?
            };
            let mut entries = Vec::new();
            if self.agent {
                for (id, label) in WINDOWS {
                    let Some(window) = result.get(id).filter(|v| v.is_object()) else {
                        continue;
                    };
                    let used = window.get("Used").and_then(decimal);
                    let limit = window.get("Quota").and_then(decimal);
                    entries.push(QuotaEntry {
                        id: id.into(),
                        source_id: id.into(),
                        label: Some(format!("Agent Plan AFP ({label})")),
                        subject: if enterprise {
                            QuotaSubject::Unknown
                        } else {
                            QuotaSubject::Account
                        },
                        model_scope: QuotaScope::All,
                        value: QuotaValue::Window(QuotaAllowance {
                            used,
                            limit,
                            remaining: limit.zip(used).map(|(l, u)| (l - u).max(0.into())),
                            used_percent: limit
                                .zip(used)
                                .filter(|(l, _)| *l > 0.into())
                                .map(|(l, u)| u / l * rust_decimal::Decimal::from(100)),
                            unit: Some("AFP".into()),
                            period_start_ms: window.get("SubscribeTime").and_then(Value::as_i64),
                            period_end_ms: window.get("ResetTime").and_then(Value::as_i64),
                            reset_behavior: QuotaResetBehavior::Periodic,
                            ..Default::default()
                        }),
                    });
                }
                if entries.is_empty() {
                    return Err(invalid_response("Ark quota reply contains no AFP windows"));
                }
            } else {
                for (id, label) in [
                    ("ShortTermUsage", "5 minutes"),
                    ("WeeklyUsage", "weekly"),
                    ("MonthlyUsage", "monthly"),
                ] {
                    if let Some(used) = result.get(id).and_then(decimal) {
                        entries.push(QuotaEntry {
                            id: id.into(),
                            source_id: id.into(),
                            label: Some(format!("Coding Plan seat ({label})")),
                            subject: QuotaSubject::Unknown,
                            model_scope: QuotaScope::All,
                            value: QuotaValue::Window(QuotaAllowance {
                                used: Some(used),
                                ..Default::default()
                            }),
                        });
                    }
                }
            }
            Ok(QuotaSnapshot {
                observed_at_ms: 0,
                entries,
            })
        })
    }
}

impl QuotaModel for Volcengine {
    fn dimensions(
        &self,
        provider: ProviderView<'_>,
        credential: CredentialView<'_>,
    ) -> Vec<QuotaDimension> {
        if !self.agent {
            if !Config::read(provider)
                .is_ok_and(|config| matches!(config.edition, Edition::Personal))
            {
                return vec![];
            }
            return CODING_WINDOWS
                .into_iter()
                .map(|(level, label)| QuotaDimension {
                    id: format!("coding_{level}"),
                    label: Some(format!("Coding Plan ({label})")),
                    scope: QuotaScope::All,
                    operations: None,
                    metric: QuotaMetric::Unit("percent".into()),
                    window: match level {
                        "session" => QuotaWindow::Rolling { seconds: 5 * 3600 },
                        "weekly" => QuotaWindow::CalendarWeek,
                        _ => QuotaWindow::CalendarMonth,
                    },
                    limit: None,
                    tracking: QuotaTracking::Reported,
                    blocking: true,
                })
                .collect();
        }
        WINDOWS
            .into_iter()
            .map(|(id, label)| QuotaDimension {
                id: id.into(),
                label: Some(format!("Agent Plan AFP ({label})")),
                scope: QuotaScope::All,
                operations: match id {
                    "AFPFiveHour" | "AFPWeekly" => Some(vec![
                        Operation::GenerateContent,
                        Operation::StreamGenerateContent,
                        Operation::CreateEmbedding,
                    ]),
                    "AFPDaily" => Some(
                        if credential
                            .metadata
                            .get("allow_paid_usage")
                            .and_then(Value::as_bool)
                            == Some(true)
                        {
                            vec![Operation::CreateImage, Operation::CreateVideo]
                        } else {
                            vec![
                                Operation::CreateImage,
                                Operation::CreateVideo,
                                Operation::CreateSpeech,
                            ]
                        },
                    ),
                    _ => None,
                },
                metric: QuotaMetric::Unit("AFP".into()),
                window: match id {
                    "AFPFiveHour" => QuotaWindow::Rolling { seconds: 5 * 3600 },
                    "AFPDaily" => QuotaWindow::CalendarDay,
                    "AFPWeekly" => QuotaWindow::CalendarWeek,
                    _ => QuotaWindow::CalendarMonth,
                },
                limit: None,
                tracking: QuotaTracking::Reported,
                blocking: true,
            })
            .collect()
    }

    fn allows_paid_usage(&self, credential: CredentialView<'_>, dimension: &str) -> bool {
        // Visual daily caps remain in force even when postpaid is enabled.
        self.agent
            && matches!(dimension, "AFPFiveHour" | "AFPWeekly" | "AFPMonthly")
            && credential
                .metadata
                .get("allow_paid_usage")
                .and_then(Value::as_bool)
                == Some(true)
    }
}

//! MiniMax-AI/cli src/client/endpoints.ts, src/types/api.ts and src/utils/quota.ts.
//! sk-api-* keys query the account balance; other keys query Token Plan usage.
//! *_usage_count historically means remaining, but newer replies can mean
//! consumed. The explicit remaining percentage disambiguates the two meanings.
use super::{DEFAULT_BASE_URL, MiniMax};
use crate::channel::{
    ChannelError, CredentialContext, OperationFuture, QuotaAllowance, QuotaBalance, QuotaEntry,
    QuotaQuery, QuotaResetBehavior, QuotaScope, QuotaSnapshot, QuotaSubject, QuotaValue,
};
use crate::channels::shared::compatible::ability::{bearer, decimal, require_success, send};
use http::Method;
use rust_decimal::Decimal;
use serde_json::Value;

fn allowance(item: &Value, weekly: bool) -> QuotaAllowance {
    let prefix = if weekly {
        "current_weekly"
    } else {
        "current_interval"
    };
    let number = |suffix: &str| item.get(format!("{prefix}_{suffix}")).and_then(decimal);
    let status = item.get(format!("{prefix}_status")).and_then(Value::as_u64);
    let percent = number("remaining_percent");
    let limit = number("total_count").filter(|v| *v > Decimal::ZERO);
    let count = number("usage_count");
    let remaining = limit.zip(count).and_then(|(total, count)| {
        if count < Decimal::ZERO || count > total {
            return None;
        }
        let Some(percent) = percent else {
            return Some(count);
        };
        let as_remaining = count / total * Decimal::ONE_HUNDRED;
        let remaining_distance = (as_remaining - percent).abs();
        let used_distance = (Decimal::ONE_HUNDRED - as_remaining - percent).abs();
        if remaining_distance.min(used_distance) > Decimal::ONE {
            return None;
        }
        Some(if used_distance < remaining_distance {
            total - count
        } else {
            count
        })
    });
    let unlimited = status.map(|status| status == 3);
    QuotaAllowance {
        used: if unlimited == Some(true) {
            None
        } else {
            limit.zip(remaining).map(|(l, r)| l - r)
        },
        limit: if unlimited == Some(true) { None } else { limit },
        remaining: if unlimited == Some(true) {
            None
        } else {
            remaining
        },
        used_percent: if unlimited == Some(true) {
            None
        } else {
            percent.map(|p| Decimal::ONE_HUNDRED - p).or_else(|| {
                limit
                    .zip(remaining)
                    .map(|(l, r)| (l - r) / l * Decimal::ONE_HUNDRED)
            })
        },
        unlimited,
        period_start_ms: item
            .get(if weekly {
                "weekly_start_time"
            } else {
                "start_time"
            })
            .and_then(Value::as_i64),
        period_end_ms: item
            .get(if weekly {
                "weekly_end_time"
            } else {
                "end_time"
            })
            .and_then(Value::as_i64),
        reset_behavior: QuotaResetBehavior::Periodic,
        ..Default::default()
    }
}

impl QuotaQuery for MiniMax {
    fn query<'a>(&'a self, ctx: CredentialContext<'a>) -> OperationFuture<'a, QuotaSnapshot> {
        Box::pin(async move {
            let key = ctx
                .credential
                .secret
                .get("api_key")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .ok_or(ChannelError::InvalidCredential)?;
            let balance = key.starts_with("sk-api-");
            let path = if balance {
                "/account/query_balance"
            } else {
                "/v1/token_plan/remains"
            };
            let base = ctx
                .provider
                .base_url
                .unwrap_or(DEFAULT_BASE_URL)
                .trim_end_matches('/');
            let (status, _, body) = send(
                ctx.client,
                Method::GET,
                &format!("{base}{path}"),
                bearer(key)?,
                None,
            )
            .await?;
            let body = require_success(status, body)?;
            let value: Value = serde_json::from_slice(&body)
                .map_err(|e| ChannelError::InvalidResponse(e.to_string()))?;
            if value
                .pointer("/base_resp/status_code")
                .is_some_and(|v| v.as_i64() != Some(0))
            {
                return Err(ChannelError::InvalidResponse(
                    "MiniMax quota request rejected".into(),
                ));
            }
            let entries = if balance {
                let remaining =
                    value
                        .get("available_amount")
                        .and_then(decimal)
                        .ok_or_else(|| {
                            ChannelError::InvalidResponse(
                                "MiniMax balance reply missing available_amount".into(),
                            )
                        })?;
                vec![QuotaEntry {
                    id: "minimax_balance".into(),
                    source_id: "minimax_balance".into(),
                    label: Some("MiniMax account balance".into()),
                    subject: QuotaSubject::Account,
                    model_scope: QuotaScope::All,
                    value: QuotaValue::Balance(QuotaBalance {
                        remaining: Some(remaining),
                        unit: None,
                    }),
                }]
            } else {
                let models = value
                    .get("model_remains")
                    .and_then(Value::as_array)
                    .ok_or_else(|| {
                        ChannelError::InvalidResponse(
                            "MiniMax quota reply missing model_remains".into(),
                        )
                    })?;
                let mut entries = Vec::new();
                for item in models {
                    let Some(model) = item.get("model_name").and_then(Value::as_str) else {
                        continue;
                    };
                    for weekly in [false, true] {
                        let window = allowance(item, weekly);
                        if window.limit.is_none()
                            && window.used_percent.is_none()
                            && window.unlimited != Some(true)
                        {
                            continue;
                        }
                        let period = if weekly { "weekly" } else { "interval" };
                        let id = format!("minimax_{model}_{period}");
                        entries.push(QuotaEntry {
                            id: id.clone(),
                            source_id: id,
                            label: Some(format!("{model} ({period})")),
                            subject: QuotaSubject::Account,
                            model_scope: if model.contains('*') {
                                QuotaScope::Unknown
                            } else {
                                QuotaScope::Models(vec![model.into()])
                            },
                            value: QuotaValue::Window(window),
                        });
                    }
                }
                entries
            };
            Ok(QuotaSnapshot {
                observed_at_ms: 0,
                entries,
            })
        })
    }
}

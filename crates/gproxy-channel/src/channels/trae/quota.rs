//! ICubeUsageService: international v1 usage and mainland v2 credits/fast
//! requests. Entitlement timestamps are Unix seconds; -1 denotes unlimited.
//! Observations remain separate per pack. They do not invent local hard limits
//! for a service with model-specific access and pay-as-you-go accounts.
use super::{
    common::{check, invalid, json, now_ms},
    config::{TraeConfig, fact},
    request,
};
use crate::channel::{
    ChannelError, CredentialContext, OperationFuture, QuotaAllowance, QuotaEntry, QuotaQuery,
    QuotaResetBehavior, QuotaScope, QuotaSnapshot, QuotaSubject, QuotaValue,
};
use crate::channels::shared::compatible::ability::{decimal, require_success, send};
use http::Method;
use rust_decimal::Decimal;
use serde_json::{Value, json};

pub(super) struct Quota;

fn millis(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .filter(|v| *v > 0)
        .and_then(|v| v.checked_mul(1000))
}

pub(super) fn entries(value: &Value) -> Result<Vec<QuotaEntry>, ChannelError> {
    check(value)?;
    let packs = value["user_entitlement_pack_list"]
        .as_array()
        .ok_or_else(|| invalid("missing entitlement packs"))?;
    let mut entries = Vec::new();
    for (index, pack) in packs.iter().enumerate() {
        let base = &pack["entitlement_base_info"];
        let id = base["entitlement_id"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| base["entitlement_id"].as_u64().map(|v| v.to_string()))
            .unwrap_or_else(|| index.to_string());
        for (name, used_key, limit_key, unit) in [
            ("basic", "basic_usage_amount", "basic_usage_limit", "usage"),
            ("bonus", "bonus_usage_amount", "bonus_usage_limit", "usage"),
            ("credits", "credits_amount", "credits_limit", "credits"),
            (
                "fast_requests",
                "premium_model_fast_amount",
                "premium_model_fast_request_limit",
                "requests",
            ),
            ("paygo", "pay_go_amount", "", "usage"),
        ] {
            if name == "bonus" && base["quota"]["no_bonus_quota"] == true {
                continue;
            }
            let used = pack["usage"].get(used_key).and_then(decimal);
            let raw_limit = base["quota"].get(limit_key).and_then(decimal);
            if used.is_none() && raw_limit.is_none() {
                continue;
            }
            let limit = raw_limit.filter(|v| *v >= Decimal::ZERO);
            let used_percent = used
                .zip(limit)
                .and_then(|(used, limit)| used.checked_div(limit))
                .and_then(|v| v.checked_mul(Decimal::ONE_HUNDRED));
            let end = millis(&base["end_time"]);
            entries.push(QuotaEntry {
                id: format!("{id}:{name}"),
                source_id: "trae_entitlements".into(),
                label: Some(format!("Trae {id} · {name}")),
                subject: QuotaSubject::Account,
                model_scope: QuotaScope::Unknown,
                value: QuotaValue::Window(QuotaAllowance {
                    used,
                    limit,
                    remaining: used
                        .zip(limit)
                        .map(|(used, limit)| (limit - used).max(Decimal::ZERO)),
                    used_percent,
                    unlimited: raw_limit.map(|v| v == Decimal::NEGATIVE_ONE),
                    unit: Some(unit.into()),
                    period_start_ms: millis(&base["start_time"]),
                    period_end_ms: end,
                    reset_behavior: if end.is_some() {
                        QuotaResetBehavior::Periodic
                    } else {
                        QuotaResetBehavior::Unknown
                    },
                }),
            });
        }
    }
    Ok(entries)
}
impl QuotaQuery for Quota {
    fn query<'a>(&'a self, ctx: CredentialContext<'a>) -> OperationFuture<'a, QuotaSnapshot> {
        Box::pin(async move {
            let config = TraeConfig::from_view(ctx.provider)?;
            let version = if config.international { "v1" } else { "v2" };
            let origin = config.quota_origin(fact(&ctx.credential, "region"));
            let (status, _, body) = send(
                ctx.client,
                Method::POST,
                &format!("{origin}/trae/api/{version}/pay/ide_user_ent_usage"),
                request::headers(&config, &ctx.credential)?,
                Some(
                    json!({"require_usage":true,"req_source":1})
                        .to_string()
                        .into_bytes(),
                ),
            )
            .await?;
            let value = json(&require_success(status, body)?)?;
            Ok(QuotaSnapshot {
                observed_at_ms: now_ms(),
                entries: entries(&value)?,
            })
        })
    }
}

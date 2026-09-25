//! Per-credential quota, riding the catalogue call:
//! `POST {base}/v1internal:fetchAvailableModels` with an empty JSON body.
//! Each entry under `models` may carry a `quotaInfo` with
//! `remainingFraction` (the share LEFT in [0, 1]) and an ISO-8601
//! `resetTime` (v3 `antigravity/quota.rs`).
//!
//! The per-model readings are two shared pools, not a bucket per model:
//! live probing (2026-09-26) moved every `MODEL_PROVIDER_GOOGLE` model
//! together on a Gemini request and every other model (Claude, GPT-OSS)
//! together on a Claude request. Each pool is a 5-hour window counted from
//! first use; while unused its `resetTime` floats at now + 5h. Internal
//! models (`chat_*`, `tab_*`) report no `resetTime` and belong to neither.
//! Which models share a pool is only known once the endpoint lists them,
//! so the pools are declared without models and `classify` takes the
//! model list from each observation. Code Assist reports no rate-limit
//! response headers, so there is no `QuotaHeaders`.

use super::{Antigravity, AntigravityConfig, catalog_request, models};
use crate::channel::{
    ChannelError, CredentialContext, CredentialView, OperationFuture, ProviderView,
    QuotaDimension, QuotaEntry, QuotaMetric, QuotaModel, QuotaQuery, QuotaScope, QuotaSnapshot,
    QuotaTracking, QuotaWindow, classify_by_id,
};
use crate::channels::shared::code_assist;
use crate::channels::shared::code_assist::quota::{iso_to_ms, used_percent};
use http::Method;
use rust_decimal::Decimal;
use serde_json::Value;
use std::borrow::Cow;

const FIVE_HOURS: i64 = 5 * 60 * 60;
const GOOGLE_PROVIDER: &str = "MODEL_PROVIDER_GOOGLE";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pool {
    Google,
    ThirdParty,
}

impl Pool {
    const ALL: [Self; 2] = [Self::Google, Self::ThirdParty];

    fn id(self) -> &'static str {
        match self {
            Self::Google => "pool_google",
            Self::ThirdParty => "pool_third_party",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Google => "Google models",
            Self::ThirdParty => "Third-party models",
        }
    }

    fn of(model_id: &str, model: &Value) -> Self {
        match model.get("modelProvider").and_then(Value::as_str) {
            Some(GOOGLE_PROVIDER) => Self::Google,
            Some(_) => Self::ThirdParty,
            // A payload without providers: Google's own models are Gemini.
            None if model_id.starts_with("gemini") => Self::Google,
            None => Self::ThirdParty,
        }
    }
}

impl QuotaModel for Antigravity {
    /// Every account has both pools; their models arrive with each reading.
    fn dimensions(&self, _: ProviderView<'_>, _: CredentialView<'_>) -> Vec<QuotaDimension> {
        Pool::ALL
            .into_iter()
            .map(|pool| QuotaDimension {
                id: pool.id().to_owned(),
                label: Some(format!("{} 5h window", pool.label())),
                scope: QuotaScope::Unknown,
                operations: None,
                metric: QuotaMetric::Unit("percent".into()),
                window: QuotaWindow::Rolling {
                    seconds: FIVE_HOURS,
                },
                limit: Some(Decimal::ONE_HUNDRED),
                tracking: QuotaTracking::Reported,
            })
            .collect()
    }

    /// A pool covers exactly the models its reading listed.
    fn classify<'d>(
        &self,
        declared: &'d [QuotaDimension],
        entry: &QuotaEntry,
    ) -> Option<Cow<'d, QuotaDimension>> {
        let dimension = classify_by_id(declared, entry)?;
        if dimension.scope != QuotaScope::Unknown
            || !matches!(entry.model_scope, QuotaScope::Models(_))
        {
            return Some(dimension);
        }
        let mut dimension = dimension.into_owned();
        dimension.scope = entry.model_scope.clone();
        Some(Cow::Owned(dimension))
    }
}

/// One entry per pool, scoped to the pool's listed models. Members move
/// together, so any one reads for the pool; the most used one is taken in
/// case a reading lands between two members' updates.
pub(super) fn entries(body: &[u8]) -> Result<Vec<QuotaEntry>, ChannelError> {
    let catalogue = models::quota_info(body)?;
    let mut pools = Pool::ALL.map(|pool| (pool, Vec::new(), None::<(Option<Decimal>, i64)>));
    for (model_id, model) in &catalogue {
        let Some(quota) = model.get("quotaInfo").filter(|value| value.is_object()) else {
            continue;
        };
        let Some(reset) = code_assist::text(quota, "resetTime").and_then(iso_to_ms) else {
            continue;
        };
        let id = code_assist::model_id(model_id).to_owned();
        let used = quota
            .get("remainingFraction")
            .and_then(Value::as_f64)
            .and_then(used_percent);
        let pool = Pool::of(&id, model);
        let (_, members, reading) = pools
            .iter_mut()
            .find(|(candidate, ..)| *candidate == pool)
            .expect("every pool is listed");
        members.push(id);
        if reading.is_none_or(|(most, _)| used > most) {
            *reading = Some((used, reset));
        }
    }
    Ok(pools
        .into_iter()
        .filter_map(|(pool, mut members, reading)| {
            let (used, reset) = reading?;
            members.sort();
            Some(code_assist::quota::window(
                pool.id().to_owned(),
                Some(pool.label().to_owned()),
                QuotaScope::Models(members),
                used,
                None,
                Some(reset),
            ))
        })
        .collect())
}

impl QuotaQuery for Antigravity {
    fn query<'a>(&'a self, context: CredentialContext<'a>) -> OperationFuture<'a, QuotaSnapshot> {
        Box::pin(async move {
            let config = AntigravityConfig::from_view(context.provider)?;
            let (url, headers, body) =
                catalog_request(&config, context.provider, &context.credential)?;
            let (status, _, bytes) = code_assist::send(
                context.client,
                Method::POST,
                &url,
                headers,
                Some(body.to_vec()),
            )
            .await?;
            if !status.is_success() {
                return Err(ChannelError::UpstreamResponse {
                    status,
                    body: bytes,
                });
            }
            Ok(QuotaSnapshot {
                // The host stamps receipt; the payload carries no time.
                observed_at_ms: 0,
                entries: entries(&bytes)?,
            })
        })
    }
}

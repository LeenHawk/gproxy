//! Per-credential quota.
//!
//! Models share quota by family, not per model: every `MODEL_PROVIDER_GOOGLE`
//! model draws on one allowance and every other model (Claude, GPT-OSS) on
//! another (live probing, 2026-09-26: a Gemini request moved every Google
//! model's reading together and nothing else; a Claude request the rest).
//! Each family can have a 5-hour and a weekly window.
//!
//! The windows come from `POST {base}/v1internal:retrieveUserQuotaSummary`
//! with the credential's `{"project": ...}` (403 `SUBSCRIPTION_REQUIRED`
//! without it). It answers per family group with one bucket per window:
//! `bucketId` `gemini-5h`, `gemini-weekly`, `3p-5h`, `3p-weekly`, each with
//! `remainingFraction` (the share LEFT) and `resetTime`. A free account
//! reports only the weekly buckets. A bucket marked `disabled` is not an
//! allowance (the 5-hour bucket once the weekly one is spent reads
//! `remainingFraction: 1, disabled: true`), so it yields no reading and the
//! weekly bucket keeps blocking.
//!
//! Which models belong to a family comes from the catalogue call,
//! `POST {base}/v1internal:fetchAvailableModels` with `{}`, whose entries
//! name their `modelProvider` and carry a `quotaInfo` for the family's
//! tightest window. When the summary is not served (404/405/501, no
//! project, or no bucket read), that `quotaInfo` stands in: the window is
//! told apart by how far off its reset is, since an idle 5-hour window
//! floats at now + 5h. Internal models (`chat_*`, `tab_*`) report no
//! `resetTime` and belong to no family. Code Assist reports no rate-limit
//! response headers, so there is no `QuotaHeaders`.

use super::{
    Antigravity, AntigravityConfig, apply_headers, base_url, catalog_request, fact, models,
};
use crate::channel::{
    ChannelError, CredentialContext, CredentialView, OperationFuture, ProviderView, QuotaDimension,
    QuotaEntry, QuotaMetric, QuotaModel, QuotaQuery, QuotaScope, QuotaSnapshot, QuotaTracking,
    QuotaWindow, classify_by_id,
};
use crate::channels::shared::code_assist;
use crate::channels::shared::code_assist::quota::{iso_to_ms, used_percent};
use http::{HeaderMap, Method, StatusCode};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use std::borrow::Cow;

const FIVE_HOURS: i64 = 5 * 60 * 60;
const WEEK: i64 = 7 * 24 * 60 * 60;
const GOOGLE_PROVIDER: &str = "MODEL_PROVIDER_GOOGLE";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Family {
    Google,
    ThirdParty,
}

impl Family {
    const ALL: [Self; 2] = [Self::Google, Self::ThirdParty];

    /// The `bucketId` prefix the summary uses.
    fn prefix(self) -> &'static str {
        match self {
            Self::Google => "gemini",
            Self::ThirdParty => "3p",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Google => "Gemini models",
            Self::ThirdParty => "Claude and GPT models",
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Span {
    FiveHours,
    Weekly,
}

impl Span {
    const ALL: [Self; 2] = [Self::FiveHours, Self::Weekly];

    fn suffix(self) -> &'static str {
        match self {
            Self::FiveHours => "5h",
            Self::Weekly => "weekly",
        }
    }

    fn seconds(self) -> i64 {
        match self {
            Self::FiveHours => FIVE_HOURS,
            Self::Weekly => WEEK,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::FiveHours => "5h window",
            Self::Weekly => "weekly window",
        }
    }
}

/// `gemini-5h`, `3p-weekly`: the summary's own bucket ids.
fn bucket_id(family: Family, span: Span) -> String {
    format!("{}-{}", family.prefix(), span.suffix())
}

fn label(family: Family, span: Span) -> String {
    format!("{} {}", family.label(), span.label())
}

impl QuotaModel for Antigravity {
    fn allows_paid_usage(&self, credential: CredentialView<'_>, dimension: &str) -> bool {
        credential
            .metadata
            .get("allow_paid_usage")
            .and_then(Value::as_bool)
            == Some(true)
            && Family::ALL.into_iter().any(|family| {
                Span::ALL
                    .into_iter()
                    .any(|span| dimension == bucket_id(family, span))
            })
    }

    /// Every account may have all four windows; their models arrive with
    /// each reading.
    fn dimensions(&self, _: ProviderView<'_>, _: CredentialView<'_>) -> Vec<QuotaDimension> {
        Family::ALL
            .into_iter()
            .flat_map(|family| Span::ALL.map(|span| (family, span)))
            .map(|(family, span)| QuotaDimension {
                id: bucket_id(family, span),
                label: Some(label(family, span)),
                scope: QuotaScope::Unknown,
                operations: None,
                metric: QuotaMetric::Unit("percent".into()),
                window: QuotaWindow::Rolling {
                    seconds: span.seconds(),
                },
                limit: Some(Decimal::ONE_HUNDRED),
                tracking: QuotaTracking::Reported,
                blocking: true,
            })
            .collect()
    }

    /// A window covers exactly the models its reading listed.
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

/// A `quotaInfo` reading: percent used, and the reset in milliseconds.
type Reading = (Option<Decimal>, i64);

/// Each family's catalogue members, and its most used `quotaInfo` reading.
/// Members move together, so any one reads for the family; the most used
/// is taken in case a reading lands between two members' updates.
struct Catalogue {
    members: [(Family, Vec<String>, Option<Reading>); 2],
}

impl Catalogue {
    fn read(body: &[u8]) -> Result<Self, ChannelError> {
        let catalogue = models::quota_info(body)?;
        let mut members = Family::ALL.map(|family| (family, Vec::new(), None));
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
            let family = Family::of(&id, model);
            let (_, ids, reading) = members
                .iter_mut()
                .find(|(candidate, ..)| *candidate == family)
                .expect("every family is listed");
            ids.push(id);
            if reading.is_none_or(|(most, _): Reading| used > most) {
                *reading = Some((used, reset));
            }
        }
        for (_, ids, _) in &mut members {
            ids.sort();
        }
        Ok(Self { members })
    }

    fn scope(&self, family: Family) -> QuotaScope {
        self.members
            .iter()
            .find(|(candidate, ..)| *candidate == family)
            .map(|(_, ids, _)| ids.clone())
            .filter(|ids| !ids.is_empty())
            .map_or(QuotaScope::Unknown, QuotaScope::Models)
    }

    /// The stand-in when there is no summary: one window per family, the
    /// span told apart by the distance to its reset.
    fn entries(&self, now_ms: i64) -> Vec<QuotaEntry> {
        self.members
            .iter()
            .filter_map(|(family, ids, reading)| {
                let (used, reset) = (*reading)?;
                let span = if reset - now_ms > (FIVE_HOURS + 10 * 60) * 1000 {
                    Span::Weekly
                } else {
                    Span::FiveHours
                };
                Some(code_assist::quota::window(
                    bucket_id(*family, span),
                    Some(label(*family, span)),
                    QuotaScope::Models(ids.clone()),
                    used,
                    None,
                    Some(reset),
                ))
            })
            .collect()
    }
}

/// The summary's buckets as windows, scoped to the catalogue's families.
/// Unknown buckets and disabled ones yield nothing.
pub(super) fn summary_entries(body: &[u8], catalogue: Option<&[u8]>) -> Vec<QuotaEntry> {
    let Ok(summary) = serde_json::from_slice::<Value>(body) else {
        return Vec::new();
    };
    let catalogue = catalogue.and_then(|body| Catalogue::read(body).ok());
    let mut entries = Vec::new();
    for bucket in summary
        .get("groups")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|group| group.get("buckets").and_then(Value::as_array))
        .flatten()
    {
        let Some(id) = code_assist::text(bucket, "bucketId") else {
            continue;
        };
        let Some((family, span)) = Family::ALL
            .into_iter()
            .flat_map(|family| Span::ALL.map(|span| (family, span)))
            .find(|(family, span)| bucket_id(*family, *span) == id)
        else {
            continue;
        };
        if bucket.get("disabled").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let used = bucket
            .get("remainingFraction")
            .and_then(Value::as_f64)
            .and_then(used_percent);
        let reset = code_assist::text(bucket, "resetTime").and_then(iso_to_ms);
        entries.push(code_assist::quota::window(
            id.to_owned(),
            Some(label(family, span)),
            catalogue
                .as_ref()
                .map_or(QuotaScope::Unknown, |catalogue| catalogue.scope(family)),
            used,
            None,
            reset,
        ));
    }
    entries
}

/// A summary endpoint the deployment does not serve.
fn unserved(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED | StatusCode::NOT_IMPLEMENTED
    )
}

impl QuotaQuery for Antigravity {
    fn query<'a>(&'a self, context: CredentialContext<'a>) -> OperationFuture<'a, QuotaSnapshot> {
        Box::pin(async move {
            let config = AntigravityConfig::from_view(context.provider)?;
            let (url, headers, body) =
                catalog_request(&config, context.provider, &context.credential)?;
            let (status, _, catalogue) = code_assist::send(
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
                    body: catalogue,
                });
            }

            let mut entries = Vec::new();
            if let Some(project) = fact(&context.credential, "project_id") {
                let token = super::access_token(&context.credential)?;
                let mut headers = HeaderMap::new();
                apply_headers(&mut headers, &config, token, true)?;
                let (status, _, summary) = code_assist::send(
                    context.client,
                    Method::POST,
                    &format!(
                        "{}/v1internal:retrieveUserQuotaSummary",
                        base_url(context.provider)
                    ),
                    headers,
                    Some(json!({ "project": project }).to_string().into_bytes()),
                )
                .await?;
                if status.is_success() {
                    entries = summary_entries(&summary, Some(&catalogue));
                } else if !unserved(status) {
                    return Err(ChannelError::UpstreamResponse {
                        status,
                        body: summary,
                    });
                }
            }
            if entries.is_empty() {
                entries = Catalogue::read(&catalogue)?.entries(code_assist::unix_now_ms());
            }
            Ok(QuotaSnapshot {
                // The host stamps receipt; the payload carries no time.
                observed_at_ms: 0,
                entries,
            })
        })
    }
}

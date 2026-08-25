mod map;

use std::collections::BTreeMap;

use bytes::Bytes;
use gproxy_store::records::{QuotaRecord, QuotaWindowKind, UsageAggregateQuery, UsageGroupBy};
use http::request::Parts;
use http::{Response, StatusCode};

use crate::dto::{QuotaWindowDto, UsageAggregateDto, UsageGroupByDto, UsageQueryDto};
use crate::handlers::util;
use crate::{AdminError, State, response};

pub(super) async fn usage(
    state: &impl State,
    parts: &Parts,
) -> Result<Response<Bytes>, AdminError> {
    let query = serde_urlencoded::from_str::<UsageQueryDto>(parts.uri.query().unwrap_or_default())
        .map_err(|error| AdminError::BadRequest(error.to_string()))?;
    let (from, to) = range(query.from, query.to)?;
    let group_by = match query.group_by {
        UsageGroupByDto::UserKey => UsageGroupBy::UserKey,
        UsageGroupByDto::User => UsageGroupBy::User,
        UsageGroupByDto::Provider => UsageGroupBy::Provider,
        UsageGroupByDto::Model => UsageGroupBy::Model,
    };
    let records = state
        .store()
        .usage_aggregate(&UsageAggregateQuery {
            from,
            to,
            group_by,
            user_key_id: query.user_key_id,
            user_id: query.user_id,
            provider_id: query.provider_id,
            model: query.model,
        })
        .await?;
    let records = records
        .into_iter()
        .map(|record| UsageAggregateDto {
            group: record.group,
            requests: record.requests,
            input_tokens: record.input_tokens,
            output_tokens: record.output_tokens,
            cached_input_tokens: record.cached_input_tokens,
            cost: record.cost.normalize().to_string(),
        })
        .collect::<Vec<_>>();
    response::json(StatusCode::OK, &records)
}

pub(super) async fn quota_windows(
    state: &impl State,
    parts: &Parts,
) -> Result<Response<Bytes>, AdminError> {
    let query = util::query(parts);
    let subject_id = util::parse_i64(util::value(&query, "subject_id"), "subject_id")?;
    let subject_kind = util::value(&query, "subject_kind");
    let snapshot = state.store().control_snapshot().await?;
    let quotas = snapshot
        .quotas
        .iter()
        .filter(|quota| quota.enabled)
        .filter(|quota| subject_id.is_none_or(|id| quota.subject_id == id))
        .filter(|quota| subject_kind.is_none_or(|kind| quota.subject_kind == kind))
        .cloned()
        .collect::<Vec<_>>();
    let values = materialize_quota_windows(state, &quotas)
        .await?
        .into_iter()
        .map(|(_, window)| window)
        .collect::<Vec<_>>();
    response::json(StatusCode::OK, &values)
}

pub(crate) async fn materialize_quota_windows(
    state: &impl State,
    quotas: &[QuotaRecord],
) -> Result<Vec<(QuotaWindowKind, QuotaWindowDto)>, AdminError> {
    let now = crate::auth::now()?;
    let mut active = state
        .store()
        .active_quota_windows()
        .await?
        .into_iter()
        .filter(|window| window.reset_at.is_none_or(|reset| reset > now))
        .map(|window| ((window.quota_id, window.window_kind), window))
        .collect::<BTreeMap<_, _>>();
    let mut values = Vec::new();
    for quota in quotas {
        for kind in map::configured_windows(quota) {
            let value = active
                .remove(&(quota.id, kind))
                .as_ref()
                .and_then(|window| map::quota_window(quota, window))
                .or_else(|| map::unstarted_window(quota, kind, now));
            if let Some(value) = value {
                values.push((kind, value));
            }
        }
    }
    Ok(values)
}

pub(super) async fn credential_cycles(
    state: &impl State,
    parts: &Parts,
) -> Result<Response<Bytes>, AdminError> {
    let query = util::query(parts);
    let from = util::parse_i64(util::value(&query, "from"), "from")?
        .ok_or_else(|| AdminError::BadRequest("from is required".into()))?;
    let to = util::parse_i64(util::value(&query, "to"), "to")?
        .ok_or_else(|| AdminError::BadRequest("to is required".into()))?;
    let (from, to) = range(from, to)?;
    let credential_id = util::parse_i64(util::value(&query, "credential_id"), "credential_id")?;
    let values = state
        .store()
        .credential_quota_cycles(credential_id, from, to)
        .await?
        .iter()
        .map(map::credential_cycle)
        .collect::<Vec<_>>();
    response::json(StatusCode::OK, &values)
}

pub(crate) fn range(from: i64, to: i64) -> Result<(i64, i64), AdminError> {
    if from >= to {
        Err(AdminError::BadRequest("from must be before to".into()))
    } else if to.saturating_sub(from) > 366 * 24 * 60 * 60 {
        Err(AdminError::BadRequest(
            "time range must not exceed 366 days".into(),
        ))
    } else {
        Ok((from, to))
    }
}

use bytes::Bytes;
use gproxy_store::records::QuotaWindowKind;
use http::{Response, StatusCode};

use super::PortalIdentity;
use crate::dto::{PortalQuotaWindowDto, PortalQuotaWindowKindDto};
use crate::{AdminError, State, handlers, response};

pub(super) async fn get(
    state: &impl State,
    identity: &PortalIdentity,
) -> Result<Response<Bytes>, AdminError> {
    let snapshot = state.store().control_snapshot().await?;
    let quotas = snapshot
        .quotas
        .iter()
        .filter(|quota| quota.enabled)
        .filter(|quota| {
            identity
                .quota_scope(&quota.subject_kind, quota.subject_id)
                .is_some()
        })
        .cloned()
        .collect::<Vec<_>>();
    let values = handlers::observability::materialize_quota_windows(state, &quotas)
        .await?
        .into_iter()
        .map(|(kind, value)| PortalQuotaWindowDto {
            scope: identity
                .quota_scope(&value.subject_kind, value.subject_id)
                .expect("portal quotas were filtered to the caller"),
            window_kind: window_kind(kind),
            window_start: value.window_start,
            reset_at: value.reset_at,
            started: value.started,
            cost_used: value.cost_used,
            cost_limit: value.cost_limit,
        })
        .collect::<Vec<_>>();
    response::json(StatusCode::OK, &values)
}

fn window_kind(kind: QuotaWindowKind) -> PortalQuotaWindowKindDto {
    match kind {
        QuotaWindowKind::Total => PortalQuotaWindowKindDto::Total,
        QuotaWindowKind::Daily => PortalQuotaWindowKindDto::Daily,
        QuotaWindowKind::Weekly => PortalQuotaWindowKindDto::Weekly,
        QuotaWindowKind::Monthly => PortalQuotaWindowKindDto::Monthly,
        QuotaWindowKind::FiveHour => PortalQuotaWindowKindDto::FiveHour,
        QuotaWindowKind::SevenDay => PortalQuotaWindowKindDto::SevenDay,
    }
}

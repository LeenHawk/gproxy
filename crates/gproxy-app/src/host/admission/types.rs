use std::time::Duration;

use gproxy_channel_api::CallerIdentity;
use serde::{Deserialize, Serialize};

pub(super) const RESERVATION_TTL: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Serialize, Deserialize)]
pub(in crate::host) struct AdmissionState {
    pub identity: IdentityState,
    pub operation: Option<String>,
    pub(super) reservations: Vec<QuotaReservation>,
}

#[derive(Serialize, Deserialize)]
pub(in crate::host) struct IdentityState {
    pub user_id: i64,
    pub user_key_id: i64,
    pub org_id: Option<i64>,
    pub team_id: Option<i64>,
}

#[derive(Serialize, Deserialize)]
pub(super) struct QuotaReservation {
    pub window_id: i64,
    pub cache_key: String,
    pub estimated_cost_micros: i64,
}

pub(super) struct CounterCharge {
    pub key: String,
    pub amount: i64,
}

pub(super) fn reservation_key(request_id: &str) -> String {
    format!("gproxy:admission:{request_id}")
}

impl From<&CallerIdentity> for IdentityState {
    fn from(identity: &CallerIdentity) -> Self {
        Self {
            user_id: identity.user_id,
            user_key_id: identity.user_key_id,
            org_id: identity.org_id,
            team_id: identity.team_id,
        }
    }
}

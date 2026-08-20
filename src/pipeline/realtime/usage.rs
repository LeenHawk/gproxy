//! Minimal usage visibility for Realtime sessions (no frame token accounting).

use rust_decimal::Decimal;

use crate::app::AppState;
use crate::billing::{self, UsageRecord};
use crate::pipeline::context::{Candidate, RequestCtx};
use crate::usage::{Ended, NormalizedUsage, UsageSource};
use crate::util::time::unix_now;

use super::RealtimeSession;

pub(super) struct UsageContext {
    state: AppState,
    request_id: String,
    at: i64,
    route_name: Option<String>,
    provider_id: i64,
    credential_id: i64,
    org_id: Option<i64>,
    team_id: Option<i64>,
    user_id: Option<i64>,
    user_key_id: Option<i64>,
    operation: String,
    kind: String,
}

impl UsageContext {
    pub(super) fn capture(state: &AppState, ctx: &RequestCtx, candidate: &Candidate) -> Self {
        let op = ctx.op.expect("realtime classified");
        let identity = ctx
            .identity
            .as_deref()
            .expect("realtime authentication ran");
        Self {
            state: state.clone(),
            request_id: ctx.request_id.clone(),
            at: unix_now(),
            route_name: ctx.route_name.clone(),
            provider_id: candidate.provider.id,
            credential_id: candidate.credential.id,
            org_id: Some(identity.user.org_id),
            team_id: identity.user.team_id,
            user_id: Some(identity.user.id),
            user_key_id: Some(identity.user_key.id),
            operation: crate::pipeline::settle::enum_str(&op.operation()),
            kind: crate::pipeline::settle::enum_str(&op.kind()),
        }
    }

    pub(super) async fn record(&self, model: &str, latency_ms: i64, ended: Ended) {
        if !self.state.cp().log_settings.enable_usage {
            return;
        }
        let usage = NormalizedUsage::default();
        let rec = UsageRecord {
            request_id: &self.request_id,
            at: self.at,
            route_name: self.route_name.as_deref(),
            provider_id: Some(self.provider_id),
            credential_id: Some(self.credential_id),
            org_id: self.org_id,
            team_id: self.team_id,
            user_id: self.user_id,
            user_key_id: self.user_key_id,
            thread_id: None,
            operation: &self.operation,
            kind: &self.kind,
            model: Some(model),
            usage: &usage,
            cost: Decimal::ZERO,
            latency_ms,
            source: UsageSource::Estimated,
            ended,
        };
        if let Err(error) = billing::record_success(self.state.persistence.as_ref(), rec).await {
            tracing::warn!(
                request_id = %self.request_id,
                error = %error,
                "realtime usage write failed"
            );
        }
    }
}

impl RealtimeSession {
    pub(crate) async fn record_usage(self, latency_ms: i64, ended: Ended) {
        let Self { model, usage, .. } = self;
        usage.record(&model, latency_ms, ended).await;
    }
}

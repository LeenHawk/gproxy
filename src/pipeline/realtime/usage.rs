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
    pricing: billing::price::Pricing,
    usage: NormalizedUsage,
    cost: Decimal,
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
            pricing: billing::pending::model_pricing(
                &state.cp(),
                candidate.provider.id,
                &candidate.upstream_model_id,
            ),
            usage: NormalizedUsage::default(),
            cost: Decimal::ZERO,
        }
    }

    pub(super) fn decorate_response_done(&mut self, text: &str) -> String {
        let Ok(mut value) = serde_json::from_str::<serde_json::Value>(text) else {
            return text.to_owned();
        };
        if value.get("type").and_then(serde_json::Value::as_str) != Some("response.done") {
            return text.to_owned();
        }
        let Some(usage) = value
            .pointer("/response/usage")
            .filter(|usage| usage.is_object())
        else {
            return text.to_owned();
        };
        let input = usage
            .get("input_tokens")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let output = usage
            .get("output_tokens")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let cached = usage
            .pointer("/input_token_details/cached_tokens")
            .or_else(|| usage.pointer("/input_tokens_details/cached_tokens"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0)
            .min(input);
        let normalized = NormalizedUsage {
            input: input.saturating_sub(cached),
            output,
            cache_read: cached,
            ..Default::default()
        };
        let cost = billing::price::cost(&normalized, &self.pricing);
        self.usage.input += normalized.input;
        self.usage.output += normalized.output;
        self.usage.cache_read += normalized.cache_read;
        self.cost += cost;
        if let Some(target) = value
            .pointer_mut("/response/usage")
            .and_then(serde_json::Value::as_object_mut)
        {
            target.insert(
                "cost".into(),
                serde_json::from_str(&cost.normalize().to_string())
                    .unwrap_or(serde_json::Value::from(0)),
            );
        }
        value.to_string()
    }

    pub(super) async fn record(&self, model: &str, latency_ms: i64, ended: Ended) {
        if !self.state.cp().log_settings.enable_usage {
            return;
        }
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
            usage: &self.usage,
            cost: self.cost,
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
    pub(crate) fn decorate_usage(&mut self, text: &str) -> String {
        self.usage.decorate_response_done(text)
    }

    pub(crate) async fn record_usage(self, latency_ms: i64, ended: Ended) {
        let Self { model, usage, .. } = self;
        usage.record(&model, latency_ms, ended).await;
    }
}

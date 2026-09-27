//! A native call, shared by passthrough and protocol conversion. Refusal
//! fallback stays inside the selected provider/credential and every send
//! goes through a fresh observed exchange. Each send is metered once, from
//! the response the channel returns (`metering`).

use super::{
    Exchange, Funnel, ObservedClient,
    metering::{Meter, UsageGate},
    prepare,
};
use crate::{AttemptContext, RewriteRuleData, api::lifecycle::now_ms};
use gproxy_channel::{ChannelBinding, ChannelError, channel::ChannelState};
use gproxy_protocol::{
    HttpBody, OperationKey, WireRequest, WireResponse, capability::CapabilityLimits,
};
use gproxy_store::entity::upstream::operation_endpoint::EndpointTransport;
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct NativeCall {
    pub funnel: Arc<Funnel>,
    pub attempt: Arc<AttemptContext>,
    pub operation: OperationKey,
    pub response_rules: Vec<Arc<RewriteRuleData>>,
    pub limits: CapabilityLimits,
    pub state: Arc<dyn ChannelState>,
    pub instance_id: Arc<str>,
}

/// Settles the exchange when the send future is dropped mid-flight. That is
/// the caller going away, so it is recorded as cancelled with no status —
/// the same as the exchange's own drop — not as an upstream failure.
struct SendGuard(Option<Arc<Exchange>>);
impl Drop for SendGuard {
    fn drop(&mut self) {
        if let Some(exchange) = self.0.take() {
            exchange.abandon();
        }
    }
}

impl NativeCall {
    /// Every native send, passthrough or the upstream leg of a conversion,
    /// asks a Chat stream for its usage here, before any channel prepares the
    /// body, so no channel has to remember to. The chunk is hidden again from
    /// a client that did not ask for it, after the exchange has observed it.
    pub async fn send(
        self,
        mut wire: WireRequest<HttpBody>,
    ) -> Result<WireResponse<HttpBody>, ChannelError> {
        let injected = super::chat_usage::opt_in(self.operation, &mut wire);
        let response = super::fallback::run(self, wire).await?;
        Ok(if injected {
            super::chat_usage::strip(response)
        } else {
            response
        })
    }

    pub async fn send_once(
        &self,
        wire: WireRequest<HttpBody>,
    ) -> Result<WireResponse<HttpBody>, ChannelError> {
        let mut attempt = self.attempt.clone();
        // Channel preparation may move model out of the body into a signed
        // URL. Pin attribution before that happens, including converted calls.
        if let HttpBody::Bytes(bytes) = &wire.body
            && let Ok(body) = serde_json::from_slice::<serde_json::Value>(bytes)
            && let Some(model) = body.get("model").and_then(serde_json::Value::as_str)
            && attempt.request.target.upstream_model.as_deref() != Some(model)
        {
            let mut request = (*attempt.request).clone();
            request.target.upstream_model = Some(model.to_owned());
            attempt = Arc::new(AttemptContext {
                request: Arc::new(request),
                attempt_id: attempt.attempt_id.clone(),
                ordinal: attempt.ordinal,
                credential: attempt.credential.clone(),
                credential_version: attempt.credential_version.clone(),
                agent_assignment: attempt.agent_assignment.clone(),
            });
        }
        let request = &attempt.request;
        let provider = &request.target.provider;
        let remaining = request
            .deadline
            .map(|at| at.saturating_duration_since(web_time::Instant::now()));
        let timeout = remaining.map_or(self.limits.operation_total, |left| {
            left.min(self.limits.operation_total)
        });
        let exchange = Exchange::new(
            self.funnel.clone(),
            attempt.clone(),
            self.operation,
            provider.channel.clone(),
            self.response_rules.clone(),
            self.limits,
            now_ms(),
        );
        // Created before the send so that a send dropped mid-flight still
        // releases the captures waiting on its reading.
        let meter = self.funnel.policy().usage.then(|| {
            let gate = UsageGate::new();
            exchange.meter_through(gate.clone());
            let request_body = match &wire.body {
                HttpBody::Bytes(bytes) => Some(bytes.clone()),
                HttpBody::Stream(_) => None,
            };
            Meter::new(self.funnel.clone(), attempt.clone(), gate, request_body)
        });
        let mut guard = SendGuard(Some(exchange.clone()));
        let observed = ObservedClient::new(attempt.credential.client.clone(), exchange.clone());
        let endpoint = provider.operation_url_for(
            self.operation,
            EndpointTransport::Http,
            request.target.upstream_model.as_deref(),
        );
        let binding = ChannelBinding::new(
            provider.channel.as_ref(),
            prepare::provider_view(provider),
            prepare::credential_view(&attempt.credential, &attempt.credential_version),
            Arc::new(observed),
        )
        .state(self.state.clone())
        .instance(self.instance_id.clone())
        .endpoint(endpoint.as_deref());
        let result = tokio::select! {
            biased;
            () = request.cancellation.cancelled() => Err(ChannelError::Host("request cancelled".into())),
            result = crate::rt::timeout(timeout, binding.send(self.operation, wire)) =>
                result.unwrap_or_else(|| Err(ChannelError::Host("upstream operation deadline exceeded".into()))),
        };
        let result = match (result, meter) {
            (Ok(response), Some(meter)) => Ok(meter.response(
                provider.channel.clone(),
                self.operation,
                self.limits.read_bytes,
                response,
            )),
            (result, _) => result,
        };
        if result.is_err() {
            exchange
                .finish(
                    gproxy_channel::channel::UsageStreamEnd::Interrupted,
                    None,
                    now_ms(),
                )
                .await;
        }
        guard.0 = None;
        result
    }
}

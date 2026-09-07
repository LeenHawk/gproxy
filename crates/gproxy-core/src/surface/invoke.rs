use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use web_time::Instant;

use bytes::Bytes;
use gproxy_channel_api::{
    BoxFuture, CallerIdentity, SurfaceInvoke, SurfaceReply, SurfaceRequest, TransportError,
};
use gproxy_protocol::SettleMode;

use crate::api::Core;
use crate::boundary::RequestCtx;
use crate::control::{ControlPlane, Plan, Pricing, Target};
use crate::error::CoreError;
use crate::funnel::error as funnel_error;
use crate::funnel::{self, FunnelCtx};
use crate::host::{CredentialId, Host, UpstreamTransport};

pub(crate) struct SurfaceCaller<'a, H: Host> {
    core: &'a Core<H>,
    target: Target,
    identity: CallerIdentity,
    plan: Plan,
    pricing: BTreeMap<CredentialId, Option<Pricing>>,
    request_id: String,
    sequence: AtomicU64,
}

impl<'a, H: Host> SurfaceCaller<'a, H> {
    pub(crate) fn new(
        core: &'a Core<H>,
        control: &dyn ControlPlane,
        target: Target,
        identity: CallerIdentity,
        plan: Plan,
        request_id: String,
    ) -> Self {
        let pricing = plan
            .targets
            .iter()
            .map(|target| {
                (
                    target.credential,
                    control.pricing(&target.provider, &target.upstream_model),
                )
            })
            .collect();
        Self {
            core,
            target,
            identity,
            plan,
            pricing,
            request_id,
            sequence: AtomicU64::new(0),
        }
    }

    fn next_id(&self, label: &str) -> String {
        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed);
        format!("{}:surface:{label}:{sequence}", self.request_id)
    }

    fn target(&self, credential: Option<CredentialId>) -> Option<Target> {
        match credential {
            None => Some(self.target.clone()),
            Some(credential) => self
                .plan
                .targets
                .iter()
                .find(|target| {
                    target.provider.id == self.target.provider.id && target.credential == credential
                })
                .cloned(),
        }
    }
}

impl<H: Host> SurfaceInvoke for SurfaceCaller<'_, H> {
    fn invoke<'a>(
        &'a self,
        request: SurfaceRequest,
    ) -> BoxFuture<'a, Result<SurfaceReply, TransportError>> {
        Box::pin(async move {
            let request_id = self.next_id(request.label);
            let started = Instant::now();
            let label = request.label;
            let key = request.key;
            let ctx = super::reply::request_ctx(&self.target, &request, request_id.clone());
            let Some(target) = self.target(request.credential) else {
                let error = CoreError::NoCredentials;
                funnel_error::request_failed_surface(&ctx, key, Some(label), &error);
                return Ok(super::reply::error(error));
            };
            let plan = Plan {
                targets: vec![target.clone()],
                budget: crate::control::FailoverBudget { max_attempts: 1 },
            };
            if let Err(error) = self
                .core
                .host
                .admit(&self.identity, &ctx, request.key, &plan)
                .await
            {
                funnel_error::request_failed_surface(&ctx, request.key, Some(label), &error);
                return Ok(super::reply::error(error));
            }
            let pricing = self
                .pricing
                .get(&target.credential)
                .cloned()
                .flatten()
                .map(|pricing| pricing.for_request(&ctx.body));
            let result = super::forward::request(
                self.core,
                &target,
                request,
                super::forward::AttemptOptions {
                    websocket: false,
                    request_id,
                    started,
                    pricing,
                    retryable: false,
                },
            )
            .await
            .and_then(|attempt| match attempt {
                super::forward::ForwardAttempt::Outcome(outcome) => Ok(outcome),
                super::forward::ForwardAttempt::Retry(_) => Err(CoreError::Internal(
                    "non-retryable surface invoke requested failover".into(),
                )),
            });
            match result {
                Ok(outcome) => super::reply::from_outcome(outcome),
                Err(error) => {
                    self.core.host.finish_admission(&ctx.request_id, None).await;
                    funnel_error::request_failed_surface(&ctx, key, Some(label), &error);
                    match error {
                        CoreError::Transport(error) => Err(error),
                        error => Ok(super::reply::error(error)),
                    }
                }
            }
        })
    }

    fn fetch_presigned<'a>(
        &'a self,
        request: http::Request<Bytes>,
    ) -> BoxFuture<'a, Result<SurfaceReply, TransportError>> {
        Box::pin(async move {
            let request_id = self.next_id("presigned");
            let started = Instant::now();
            let (parts, body) = request.into_parts();
            let ctx = RequestCtx {
                request_id: request_id.clone(),
                client_ip: None,
                method: parts.method.clone(),
                path: parts.uri.path().into(),
                query: parts.uri.query().map(str::to_owned),
                headers: parts.headers.clone(),
                body: body.clone(),
                upgrade: false,
                force_model_refresh: false,
                mode: crate::boundary::RoutingMode::Scoped {
                    provider: self.target.provider.name.clone(),
                },
            };
            let plan = Plan {
                targets: vec![self.target.clone()],
                budget: crate::control::FailoverBudget { max_attempts: 1 },
            };
            if let Err(error) = self
                .core
                .host
                .admit(&self.identity, &ctx, None, &plan)
                .await
            {
                funnel_error::request_failed_surface(&ctx, None, Some("presigned"), &error);
                return Ok(super::reply::error(error));
            }
            let traffic_policy = self
                .core
                .channels
                .get(&self.target.provider.channel)
                .map(|channel| {
                    channel
                        .descriptor()
                        .traffic_policy
                        .effective_traffic_policy(&self.target.provider.settings)
                })
                .transpose()
                .map_err(TransportError::Interrupted)?;
            let mut facts = FunnelCtx {
                upstream_started_at_ms: Some(crate::quota::now_ms()),
                request_id,
                target: self.target.clone(),
                credential_version: None,
                source_key: None,
                key: None,
                source_framing: gproxy_protocol::StreamFraming::Sse,
                target_framing: gproxy_protocol::StreamFraming::Sse,
                settle: SettleMode::Free,
                pricing: None,
                pricing_control: None,
                usage_channel: None,
                started,
                upstream_url: Some(parts.uri.to_string()),
                request_method: Some(parts.method.clone()),
                request_body: body.clone(),
                request_headers: Some(parts.headers.clone()),
                client_headers: parts.headers.clone(),
                requested_model: None,
                response_headers: None,
                dedupe_key: None,
                owner_user_id: Some(self.identity.user_id),
                resource: None,
                admitted: true,
                surface_label: Some("presigned"),
                traffic_policy,
                traffic_blacklist: Some(self.target.provider.traffic_blacklist.clone()),
            };
            let request = http::Request::from_parts(parts, body);
            let response = match self.core.host.transport().send(request).await {
                Ok(response) => response,
                Err(error) => {
                    funnel_error::attempt_transport(self.core.host.as_ref(), &facts, &error).await;
                    self.core.host.finish_admission(&ctx.request_id, None).await;
                    funnel_error::request_transport_failed(&ctx, None, Some("presigned"), &error);
                    return Err(error);
                }
            };
            facts.response_headers = Some(response.headers().clone());
            let (parts, body) = response.into_parts();
            let disposition = if parts.status.is_success() {
                gproxy_channel_api::Disposition::Success
            } else {
                gproxy_channel_api::Disposition::Terminal
            };
            super::reply::from_outcome(
                funnel::free_streaming(
                    self.core.host.clone(),
                    facts,
                    parts.status,
                    parts.headers,
                    body,
                    disposition,
                )
                .await,
            )
        })
    }

    fn wait<'a>(&'a self, duration: std::time::Duration) -> BoxFuture<'a, ()> {
        self.core.host.wait(duration)
    }
}

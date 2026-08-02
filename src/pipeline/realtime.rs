//! Admission and upstream handshake failover for Realtime passthrough sessions.

use bytes::Bytes;
use http::Method;

use crate::app::AppState;
use crate::channel::{PrepareCtx, PreparedRequest};
use crate::health::CredAdmit;
use crate::health::config::breaker_config;
use crate::http::client::ConduitSocket;
use crate::pipeline::context::RequestCtx;
use crate::pipeline::error::PipelineError;
use crate::pipeline::{auth, candidate, classify, health_hooks, ingress, transform};

pub(crate) struct RealtimeSession {
    pub socket: Box<dyn ConduitSocket>,
    pub provider: String,
    pub channel: String,
    pub model: String,
    pub request_id: String,
}

pub(crate) async fn open(
    state: &AppState,
    mut ctx: RequestCtx,
) -> Result<RealtimeSession, PipelineError> {
    let prepared = {
        let cp = state.cp();
        ctx.identity = Some(auth::authenticate(&cp, &ctx.headers, ctx.query.as_deref())?);
        ingress::apply_global_blacklist(&mut ctx);
        let classified = classify::classify(&ctx.method, &ctx.path, &ctx.headers, &ctx.body)?;
        ctx.op = Some(classified.op);
        ctx.stream = true;
        ctx.body_model = crate::channel::realtime_websocket::query_model(ctx.query.as_deref());
        candidate::prepare(&cp, &ctx, classified.op)?
    };
    let request = match prepared {
        candidate::Prepared::Candidates(request) => *request,
        candidate::Prepared::ScopedModels(_) => return Err(PipelineError::RuleUnsupported),
    };
    if let Some(route) = request.route_name() {
        ctx.route_name = Some(route.to_owned());
    }
    let identity = ctx
        .identity
        .as_deref()
        .expect("realtime authentication ran");
    let admitted = request.admit(state, identity, true).await?;
    open_candidates(state, &ctx, admitted.candidates).await
}

async fn open_candidates(
    state: &AppState,
    ctx: &RequestCtx,
    candidates: Vec<crate::pipeline::Candidate>,
) -> Result<RealtimeSession, PipelineError> {
    let source = ctx.op.expect("realtime classified");
    let mut attempts = 0;
    let mut last_error = None;
    let mut eligible = false;
    for candidate in candidates {
        let channel_id = candidate.provider.channel.as_str();
        if !matches!(channel_id, "openai" | "codex") {
            continue;
        }
        let passthrough = {
            let cp = state.cp();
            matches!(
                transform::plan_for(&cp, candidate.provider.id, source),
                Ok(transform::TransformPlan::Passthrough)
            )
        };
        if !passthrough {
            continue;
        }
        eligible = true;
        if attempts >= state.config.max_attempts {
            break;
        }
        let config = breaker_config(&candidate.provider.settings_json);
        if state.health.admit_credential_model(
            candidate.credential.id,
            &candidate.upstream_model_id,
            &config,
            crate::util::time::unix_now(),
        ) == CredAdmit::No
        {
            continue;
        }
        let Some(channel) = state.channels.get(channel_id) else {
            last_error = Some(PipelineError::UnknownChannel(channel_id.to_owned()));
            continue;
        };
        let opened = match state.cipher.open(&candidate.credential.secret_json) {
            Ok(secret) => secret,
            Err(_) => {
                last_error = Some(PipelineError::Channel(
                    crate::channel::ChannelError::InvalidCredential(
                        "sealed secret unreadable".into(),
                    ),
                ));
                continue;
            }
        };
        let secret = match state
            .ensure_fresh_credential(
                &channel,
                &candidate.credential,
                &candidate.provider,
                opened,
                false,
            )
            .await
        {
            Ok(secret) => secret,
            Err(error) => {
                health_hooks::record_failure(state, &ctx.request_id, &candidate);
                last_error = Some(PipelineError::Channel(error));
                continue;
            }
        };
        attempts += 1;
        match open_candidate(state, ctx, &candidate, &channel, &secret).await {
            Ok(socket) => {
                health_hooks::record_attempt(
                    state,
                    &ctx.request_id,
                    &candidate,
                    &crate::channel::Disposition::Success,
                    None,
                );
                return Ok(RealtimeSession {
                    socket,
                    provider: candidate.provider.name.clone(),
                    channel: channel_id.to_owned(),
                    model: candidate.upstream_model_id.clone(),
                    request_id: ctx.request_id.clone(),
                });
            }
            Err(error) => {
                health_hooks::record_failure(state, &ctx.request_id, &candidate);
                last_error = Some(error);
            }
        }
    }
    Err(last_error.unwrap_or(if eligible {
        PipelineError::AllAttemptsFailed
    } else {
        PipelineError::RuleUnsupported
    }))
}

async fn open_candidate(
    state: &AppState,
    ctx: &RequestCtx,
    candidate: &crate::pipeline::Candidate,
    channel: &std::sync::Arc<dyn crate::channel::Channel>,
    secret: &serde_json::Value,
) -> Result<Box<dyn ConduitSocket>, PipelineError> {
    let query = crate::channel::realtime_websocket::rewrite_model_query(
        ctx.query.as_deref(),
        &candidate.upstream_model_id,
    )?;
    let prepared = channel.prepare(PrepareCtx {
        secret,
        provider_settings: &candidate.provider.settings_json,
        op: ctx.op.expect("realtime classified"),
        stream: true,
        upstream_model_id: &candidate.upstream_model_id,
        method: Method::GET,
        path: &ctx.path,
        query: Some(&query),
        headers: &ctx.headers,
        body: Bytes::new(),
    })?;
    let request = match prepared {
        PreparedRequest::Direct(request) => request,
        _ => {
            return Err(PipelineError::Transport(
                "realtime channel did not build a direct websocket request".into(),
            ));
        }
    };
    let client = state
        .upstream_client_for_credential(channel, &candidate.credential, &candidate.provider)
        .map_err(|error| PipelineError::Transport(error.to_string()))?;
    client
        .open_websocket(request)
        .await
        .map_err(|error| PipelineError::Transport(error.to_string()))
}

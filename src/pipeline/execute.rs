//! The generic request orchestrator (§6.3). Sequences the already-separated
//! steps for both routing modes; stream & non-stream share every step and
//! diverge only at the body tail inside [`failover`](crate::pipeline::failover).

use std::sync::Arc;

use tracing::Instrument;

use crate::app::AppState;
use crate::billing::pending;
use crate::pipeline::context::{Candidate, RequestCtx, RoutingMode};
use crate::pipeline::error::PipelineError;
use crate::pipeline::local_ops::{self, ModelEntry};
use crate::pipeline::outcome::{ExecOutcome, ResponseBody};
use crate::pipeline::{
    auth, authz, balance, capture, classify, failover, ingress, preprocess, route,
};
use crate::protocol::Operation;
use crate::util::time::unix_now;

/// Drive one request to an [`ExecOutcome`], wrapped in a per-request tracing
/// span (§15.2) carrying `request_id` and — recorded as they resolve —
/// `op` / `kind` / `route` / `provider`.
pub async fn execute(state: &AppState, ctx: RequestCtx) -> Result<ExecOutcome, PipelineError> {
    let span = tracing::info_span!(
        "request",
        request_id = %ctx.request_id,
        op = tracing::field::Empty,
        kind = tracing::field::Empty,
        route = tracing::field::Empty,
        provider = tracing::field::Empty,
    );
    // §8-D downstream capture: snapshot the inbound wire facts BEFORE run()
    // (the ingress blacklist strips client creds in place); the row is written
    // below once the final status is known. None when the toggle is off.
    let downstream = capture::downstream_precapture(state, &ctx);
    let result = run(state, ctx).instrument(span).await;
    if let Some(cap) = downstream {
        // §8-D response body (fold-in for non-streaming; streamed bodies are
        // backfilled by `settle` since they aren't materialized here).
        let want_body = state.cp().log_settings.enable_downstream_log_body;
        let (status, resp_body): (http::StatusCode, Option<bytes::Bytes>) = match &result {
            Ok(o) => {
                let b = match &o.body {
                    #[cfg(not(target_arch = "wasm32"))]
                    ResponseBody::Stream(_) => None,
                    ResponseBody::Full(b) if want_body => Some(b.clone()),
                    ResponseBody::Full(_) => None,
                };
                (o.status, b)
            }
            Err(e) => (
                e.status(),
                want_body.then(|| bytes::Bytes::from(e.error_body_json())),
            ),
        };
        capture::log_downstream(state, cap, status, resp_body.as_deref()).await;
    }
    result
}

/// Inner orchestrator (§6.3). Sequences the already-separated steps for both
/// routing modes; stream & non-stream share every step and diverge only at the
/// body tail inside [`failover`](crate::pipeline::failover).
async fn run(state: &AppState, mut ctx: RequestCtx) -> Result<ExecOutcome, PipelineError> {
    let cp = state.cp();

    // auth (401 short-circuit before any upstream candidate is built)
    ctx.identity = Some(auth::authenticate(&cp, &ctx.headers, ctx.query.as_deref())?);

    // Part 1 — global blacklist: strip caller creds / cookies / hop-by-hop once,
    // centrally, after auth and before any channel sees the request (the
    // per-channel allow-list runs later in Channel::prepare).
    ingress::apply_global_blacklist(&mut ctx);

    // classify
    let classified = classify::classify(&ctx.method, &ctx.path, &ctx.headers, &ctx.body)?;
    ctx.op = Some(classified.op);
    ctx.stream = classified.stream;
    let span = tracing::Span::current();
    span.record("op", tracing::field::debug(classified.op.operation));
    span.record("kind", tracing::field::debug(classified.op.kind));

    // Aggregated models surface (§6.3): the gateway's own view — alias + route
    // names — served before preprocess/route (there is no model to route) and
    // never touching an upstream.
    if matches!(ctx.mode, RoutingMode::Aggregated)
        && matches!(
            classified.op.operation,
            Operation::ListModels | Operation::GetModel
        )
    {
        return aggregated_models(&cp, &ctx);
    }

    // resolve candidates per routing mode, with authz (§8-C permission +
    // limits) on the canonical name BEFORE any candidate is built. The
    // snapshot guard `cp` is held across authorize's await AND
    // balance::candidates' await (sticky-pin cache get/set) — both are
    // sub-millisecond cache ops, not the upstream call the M2 invariant
    // guards against. Each arm also yields the §17 pre-deduct estimate
    // (computed here, where `cp` and the resolved route/provider are alive);
    // the estimate-aware quota gate runs once, below, at the common point.
    let (candidates, est_micros) = match &ctx.mode {
        RoutingMode::Aggregated => {
            let route_name = preprocess::preprocess(&cp, &ctx)?;
            span.record("route", route_name.as_str());
            let resolved = route::route(&cp, &route_name)?;
            let identity = ctx.identity.as_ref().expect("auth ran first");
            authz::authorize(&cp, state.cache.as_ref(), identity, &route_name, unix_now()).await?;
            // best-effort estimate priced at the FIRST enabled member's model
            let est = resolved
                .members
                .first()
                .map(|m| estimate(&cp, &ctx, m.provider_id, &m.upstream_model_id))
                .unwrap_or(0);
            let cands = balance::candidates(
                &cp,
                resolved,
                state.health.as_ref(),
                state.cache.as_ref(),
                Some(identity.user_key.id),
            )
            .await?;
            ctx.route_name = Some(route_name);
            (cands, est)
        }
        RoutingMode::Scoped { provider } => {
            let provider = cp
                .providers_by_name
                .get(provider.as_str())
                .filter(|p| p.enabled)
                .ok_or_else(|| PipelineError::UnknownProvider(provider.clone()))?;
            span.record("provider", provider.name.as_str());
            let identity = ctx.identity.as_ref().expect("auth ran first");
            authz::authorize(
                &cp,
                state.cache.as_ref(),
                identity,
                &provider.name,
                unix_now(),
            )
            .await?;
            let cands = scoped_candidates(&cp, provider, &ctx)?;
            // scoped: priced at the scoped provider's (variant-stripped) model
            let est = cands
                .first()
                .map(|c| estimate(&cp, &ctx, provider.id, &c.upstream_model_id))
                .unwrap_or(0);
            (cands, est)
        }
    };

    // §17 quota admission — the single quota gate on the request path: the
    // estimate must fit every quota-bearing scope's remaining budget. Runs
    // before pending::charge and before any upstream byte (the first one is
    // sent inside failover::run_failover).
    let identity = ctx.identity.as_ref().expect("auth ran first");
    authz::precheck_quota(&cp, state.cache.as_ref(), identity, est_micros).await?;

    // §17 pre-deduct: admission passed — charge the estimate to every
    // quota-bearing scope now, before any upstream byte. Settle refunds
    // the exact amount; the error path below refunds when nothing settles.
    let quota_scopes = if est_micros > 0 {
        authz::quota_scopes(&cp, identity)
    } else {
        Vec::new()
    };
    let pending_micros = if quota_scopes.is_empty() {
        0
    } else {
        est_micros
    };
    pending::charge(state.cache.as_ref(), &quota_scopes, pending_micros).await;
    ctx.pending_micros = pending_micros;

    // Candidates own their Arcs; drop the snapshot guard before the (possibly
    // long-lived, streaming) upstream call so it doesn't pin the old snapshot
    // across an invalidation/swap.
    drop(cp);

    let result = failover::run_failover(state, &ctx, &candidates).await;
    // Only a 2xx content response attaches a SettleCtx (whose settle refunds
    // the pending). Everything else — pipeline error, all-candidates-failed,
    // or a relayed permanent 4xx — must refund here. A crash in between
    // self-heals via the 15-minute pending TTL.
    if !matches!(&result, Ok(o) if o.status.is_success()) {
        pending::refund(state.cache.as_ref(), &quota_scopes, pending_micros).await;
    }
    result
}

/// §17 pre-deduct estimate in micro-dollars for the billable ops
/// (content-generation + embeddings = body chars ×1 as input tokens; image
/// generation = `n` × the per-image rate). Other ops / no pricing → 0
/// (pre-deduct skipped). Settle refunds the exact amount.
fn estimate(
    cp: &crate::app::snapshot::ControlPlaneSnapshot,
    ctx: &RequestCtx,
    provider_id: i64,
    model_id: &str,
) -> i64 {
    let Some(op) = ctx.op else { return 0 };
    match op.operation {
        // Token-priced: estimate the body char count as input tokens (×1).
        Operation::GenerateContent
        | Operation::StreamGenerateContent
        | Operation::CreateEmbedding => {
            let pricing = pending::model_pricing(cp, provider_id, model_id);
            pending::estimate_micros(&pricing, ctx.body.len())
        }
        // Image generation: `n` images at the requested size/quality rate.
        Operation::CreateImage | Operation::EditImage => {
            let req: Option<serde_json::Value> = serde_json::from_slice(&ctx.body).ok();
            let field = |k: &str| {
                req.as_ref()
                    .and_then(|r| r.get(k))
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            };
            let n = req
                .as_ref()
                .and_then(|r| r.get("n"))
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(1);
            let pricing_json = cp
                .models_by_provider
                .get(&provider_id)
                .and_then(|ms| ms.iter().find(|m| m.model_id == model_id))
                .and_then(|m| m.pricing_json.clone());
            let rate = crate::billing::price::image_rate(
                pricing_json.as_ref(),
                field("size").as_deref(),
                field("quality").as_deref(),
            );
            pending::to_micros(rust_decimal::Decimal::from(n) * rate)
        }
        // models / count / compact / etc. are never billed → no pre-deduct.
        _ => 0,
    }
}

/// Scoped mode (`/{provider}/v1/...`): bypass routing, hit the named provider
/// directly. Provider must exist + be enabled; model validation is lax (M1
/// behavior kept) — but a known variant suffix strips to its base as the
/// upstream model (§8-B), with the body/path rewrite done downstream by
/// `transform::request_parts`.
fn scoped_candidates(
    cp: &crate::app::snapshot::ControlPlaneSnapshot,
    provider: &Arc<crate::store::persistence::records::Provider>,
    ctx: &RequestCtx,
) -> Result<Vec<Candidate>, PipelineError> {
    // Requested name: body peek, else path-embedded (gemini `models/{id}:verb`,
    // models GETs). Process filters still see this ORIGINAL full name (§8-B) —
    // only upstream_model_id is stripped.
    let requested = classify::peek_model(&ctx.body)
        .or_else(|| classify::path_model_id(&ctx.path))
        .unwrap_or_default();
    let model = cp
        .variant_base_by_provider
        .get(&provider.id)
        .and_then(|idx| idx.get(&requested))
        .cloned()
        .unwrap_or(requested);
    let creds = cp
        .credentials_by_provider
        .get(&provider.id)
        .filter(|c| !c.is_empty())
        .ok_or(PipelineError::NoCredentials)?;

    // Scoped mode has no route, so the member breaker is skipped; carry the
    // plain provider breaker config for the credential breaker.
    let breaker_cfg = crate::health::config::breaker_config(&provider.settings_json);
    Ok(creds
        .iter()
        .map(|cred| Candidate {
            provider: Arc::clone(provider),
            credential: Arc::clone(cred),
            upstream_model_id: model.clone(),
            member_id: None,
            breaker_cfg: breaker_cfg.clone(),
        })
        .collect())
}

/// Serve aggregated ListModels/GetModel from the snapshot: alias names + route
/// names ARE the model list, filtered to what the caller's permission union
/// allows. Non-permitted GetModel 404s identically to missing (no existence
/// leak).
fn aggregated_models(
    cp: &crate::app::snapshot::ControlPlaneSnapshot,
    ctx: &RequestCtx,
) -> Result<ExecOutcome, PipelineError> {
    let op = ctx.op.expect("classified");
    let family = op.provider_family();
    let identity = ctx.identity.as_ref().expect("auth ran first");
    let known = |id: &str| cp.alias_to_route.contains_key(id) || cp.routes_by_name.contains_key(id);

    let body = match op.operation {
        Operation::ListModels => {
            let mut ids: Vec<&String> = cp
                .alias_to_route
                .keys()
                .chain(cp.routes_by_name.keys())
                .filter(|id| authz::permitted(cp, identity, id))
                .collect();
            ids.sort();
            ids.dedup();
            let entries: Vec<ModelEntry> = ids
                .into_iter()
                .map(|id| ModelEntry {
                    id: id.clone(),
                    display_name: None,
                })
                .collect();
            local_ops::render_model_list(family, &entries)
        }
        _ => {
            let id = classify::path_model_id(&ctx.path).ok_or(PipelineError::UnsupportedPath)?;
            if !known(&id) || !authz::permitted(cp, identity, &id) {
                return Err(PipelineError::UnknownRoute(id));
            }
            local_ops::render_model(
                family,
                &ModelEntry {
                    id,
                    display_name: None,
                },
            )
        }
    };

    Ok(local_ops::json_outcome(http::StatusCode::OK, body))
}

//! The generic request orchestrator (§6.3). Sequences the already-separated
//! steps for both routing modes; stream & non-stream share every step and
//! diverge only at the body tail inside [`failover`](crate::pipeline::failover).

use std::sync::Arc;

#[cfg(target_arch = "wasm32")]
use bytes::Bytes;
use tracing::Instrument;

use crate::app::AppState;
use crate::billing::pending;
use crate::pipeline::context::{Candidate, RequestCtx, RoutingMode};
use crate::pipeline::error::PipelineError;
use crate::pipeline::local_ops::{self, ModelEntry};
use crate::pipeline::outcome::{ExecOutcome, ResponseBody};
use crate::pipeline::{
    auth, authz, balance, capture, classify, failover, ingress, preprocess, route, transform,
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
    ingress::normalize_multipart_form_body(&mut ctx)?;

    // classify
    let classified = classify::classify(&ctx.method, &ctx.path, &ctx.headers, &ctx.body)?;
    ctx.op = Some(classified.op);
    ctx.stream = classified.stream;
    let span = tracing::Span::current();
    span.record("op", tracing::field::debug(classified.op.operation));
    span.record("kind", tracing::field::debug(classified.op.kind));

    // Aggregated models surface (§6.3): apply each permitted provider's refresh
    // policy, fall back to persisted catalogues, then merge public aliases and
    // routes. Served before preprocess/route because there is no single model
    // to route.
    if matches!(ctx.mode, RoutingMode::Aggregated)
        && matches!(
            classified.op.operation,
            Operation::ListModels | Operation::GetModel
        )
    {
        drop(cp);
        return aggregated_models(state, &ctx).await;
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
            let model = preprocess::preprocess(&cp, &ctx)?;
            if cp.routes_by_name.contains_key(&model) {
                span.record("route", model.as_str());
                let resolved = route::route(&cp, &model)?;
                let identity = ctx.identity.as_ref().expect("auth ran first");
                authz::authorize(&cp, state.cache.as_ref(), identity, &model, unix_now()).await?;
                let cands = balance::candidates(
                    &cp,
                    resolved,
                    state.health.as_ref(),
                    state.cache.as_ref(),
                    Some(identity.user_key.id),
                )
                .await?;
                let est = cands
                    .iter()
                    .map(|c| estimate(&cp, &ctx, c.provider.id, &c.upstream_model_id))
                    .max()
                    .unwrap_or(0);
                ctx.route_name = Some(model);
                (cands, est)
            } else if let Some((provider_name, upstream_model_id)) =
                preprocess::split_provider_model(&model)
            {
                let provider = cp
                    .providers_by_name
                    .get(provider_name)
                    .filter(|p| p.enabled)
                    .ok_or_else(|| PipelineError::UnknownProvider(provider_name.to_owned()))?;
                span.record("provider", provider.name.as_str());
                let identity = ctx.identity.as_ref().expect("auth ran first");
                let authorized_model =
                    preprocess::apply_provider_alias(&cp, &provider.name, upstream_model_id);
                authz::authorize_provider_model(
                    &cp,
                    state.cache.as_ref(),
                    identity,
                    &provider.name,
                    &authorized_model,
                    unix_now(),
                )
                .await?;
                let cands = provider_candidates(&cp, provider, upstream_model_id)?;
                let est = cands
                    .iter()
                    .map(|c| estimate(&cp, &ctx, provider.id, &c.upstream_model_id))
                    .max()
                    .unwrap_or(0);
                (cands, est)
            } else {
                return Err(PipelineError::UnknownRoute(model));
            }
        }
        RoutingMode::Scoped { provider } => {
            let provider = cp
                .providers_by_name
                .get(provider.as_str())
                .filter(|p| p.enabled)
                .ok_or_else(|| PipelineError::UnknownProvider(provider.clone()))?;
            span.record("provider", provider.name.as_str());
            let identity = ctx.identity.as_ref().expect("auth ran first");
            if classified.op.operation == Operation::ListModels {
                authz::authorize_provider_listing(
                    &cp,
                    state.cache.as_ref(),
                    identity,
                    &provider.name,
                    unix_now(),
                )
                .await?;
                let provider = Arc::clone(provider);
                let identity = Arc::clone(identity);
                let source = classified.op;
                drop(cp);
                return Ok(crate::pipeline::model_catalog::serve_scoped(
                    state, provider, identity, source,
                )
                .await);
            }
            if let Some(requested) = preprocess::requested_model(&ctx) {
                let authorized_model =
                    preprocess::apply_provider_alias(&cp, &provider.name, &requested);
                authz::authorize_provider_model(
                    &cp,
                    state.cache.as_ref(),
                    identity,
                    &provider.name,
                    &authorized_model,
                    unix_now(),
                )
                .await?;
            } else {
                authz::authorize(
                    &cp,
                    state.cache.as_ref(),
                    identity,
                    &provider.name,
                    unix_now(),
                )
                .await?;
            }
            let cands = scoped_candidates(&cp, provider, &ctx)?;
            // scoped: priced at the scoped provider's (variant-stripped) model
            let est = cands
                .iter()
                .map(|c| estimate(&cp, &ctx, provider.id, &c.upstream_model_id))
                .max()
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

    let synthesize_stream = ctx.stream
        && candidates.iter().any(|candidate| {
            matches!(
                transform::plan_for(&cp, candidate.provider.id, ctx.op.expect("classified")),
                Ok(transform::TransformPlan::SynthesizeStream { .. })
            )
        });

    // Candidates own their Arcs; drop the snapshot guard before the (possibly
    // long-lived, streaming) upstream call so it doesn't pin the old snapshot
    // across an invalidation/swap.
    drop(cp);

    #[cfg(not(target_arch = "wasm32"))]
    if synthesize_stream {
        return Ok(crate::pipeline::stream::synthetic_outcome(
            state.clone(),
            ctx,
            candidates,
            quota_scopes,
            pending_micros,
        ));
    }

    #[cfg(not(target_arch = "wasm32"))]
    let result = failover::run_failover(state, &ctx, &candidates).await;
    #[cfg(target_arch = "wasm32")]
    let mut result = failover::run_failover(state, &ctx, &candidates).await;
    // Only a 2xx content response attaches a SettleCtx (whose settle refunds
    // the pending). Everything else — pipeline error, all-candidates-failed,
    // or a relayed permanent 4xx — must refund here. A crash in between
    // self-heals via the 15-minute pending TTL.
    if !matches!(&result, Ok(o) if o.status.is_success()) {
        pending::refund(state.cache.as_ref(), &quota_scopes, pending_micros).await;
    }

    // Edge currently buffers all response bodies. Preserve the synthetic
    // route's wire contract even though live keepalives require the native
    // streaming runtime.
    #[cfg(target_arch = "wasm32")]
    if synthesize_stream
        && let Ok(outcome) = &mut result
        && outcome.status.is_success()
        && let ResponseBody::Full(body) = &outcome.body
    {
        let kind = match ctx.op.expect("classified").kind {
            crate::protocol::OperationKind::ContentGeneration(kind) => kind,
            crate::protocol::OperationKind::Provider(_) => unreachable!("synthetic content"),
        };
        let gemini_json = kind == crate::protocol::ContentGenerationKind::GeminiGenerateContent
            && !ctx
                .query
                .as_deref()
                .is_some_and(|query| query.split('&').any(|part| part == "alt=sse"));
        let encoded = if gemini_json {
            let value: serde_json::Value = serde_json::from_slice(body).map_err(|error| {
                PipelineError::TransformResponse(crate::transform::TransformError::InvalidInput {
                    reason: format!("synthetic gemini response is not JSON: {error}"),
                })
            })?;
            Bytes::from(serde_json::to_vec(&vec![value]).map_err(|error| {
                PipelineError::TransformResponse(crate::transform::TransformError::Serialization {
                    reason: error.to_string(),
                })
            })?)
        } else {
            Bytes::from(
                crate::transform::stream_adapter::synthesize_sse(kind, body)
                    .map_err(PipelineError::TransformResponse)?,
            )
        };
        outcome.body = ResponseBody::Full(encoded);
        outcome.headers.remove(http::header::CONTENT_LENGTH);
        outcome.headers.insert(
            http::header::CONTENT_TYPE,
            if gemini_json {
                http::HeaderValue::from_static("application/json")
            } else {
                http::HeaderValue::from_static("text/event-stream")
            },
        );
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
    if matches!(
        transform::plan_for(cp, provider_id, op),
        Ok(transform::TransformPlan::Local)
    ) {
        return 0;
    }
    match op.operation {
        // Token-priced: estimate the body char count as input tokens (×1).
        Operation::GenerateContent
        | Operation::StreamGenerateContent
        | Operation::CreateEmbedding => {
            let pricing = pending::model_pricing(cp, provider_id, model_id);
            pending::estimate_micros(&pricing, ctx.body.len())
        }
        // Image generation: `n` images at the configured per-image price.
        Operation::CreateImage | Operation::EditImage => {
            let req: Option<serde_json::Value> = serde_json::from_slice(&ctx.body).ok();
            let n = req
                .as_ref()
                .and_then(|r| r.get("n"))
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(1);
            let resolved = pending::resolve_pricing(cp, provider_id, model_id);
            pending::to_micros(rust_decimal::Decimal::from(n) * resolved.pricing.image)
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
    provider_candidates(cp, provider, &requested)
}

fn provider_candidates(
    cp: &crate::app::snapshot::ControlPlaneSnapshot,
    provider: &Arc<crate::store::persistence::records::Provider>,
    requested: &str,
) -> Result<Vec<Candidate>, PipelineError> {
    let requested = preprocess::apply_provider_alias(cp, &provider.name, requested);
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

/// Serve aggregated ListModels/GetModel: provider catalogues are refreshed in
/// parallel with persisted fallback, then public route names, aliases, and
/// provider models are merged and permission-filtered. Non-permitted GetModel
/// 404s identically to missing (no existence leak).
async fn aggregated_models(
    state: &AppState,
    ctx: &RequestCtx,
) -> Result<ExecOutcome, PipelineError> {
    let op = ctx.op.expect("classified");
    let family = op.provider_family();
    let identity = ctx.identity.as_ref().expect("auth ran first");

    let body = match op.operation {
        Operation::ListModels => {
            // Snapshot only long enough to select the permitted provider
            // namespaces. Never pin it across the parallel upstream refreshes.
            let providers: Vec<Arc<crate::store::persistence::records::Provider>> = {
                let cp = state.cp();
                cp.providers_by_name
                    .values()
                    .filter(|provider| {
                        provider.enabled
                            && authz::provider_listing_permitted(&cp, identity, &provider.name)
                    })
                    .cloned()
                    .collect()
            };
            // Eligible providers refresh concurrently; local/disabled refresh
            // providers read their persisted catalogues without upstream I/O.
            let catalogues = futures_util::future::join_all(providers.iter().map(|provider| {
                crate::pipeline::model_catalog::models_for_request(state, provider, op)
            }))
            .await;

            let cp = state.cp();
            let mut ids: Vec<String> = Vec::new();
            ids.extend(
                cp.routes_by_name
                    .keys()
                    .filter(|id| authz::permitted(&cp, identity, id))
                    .cloned(),
            );
            if let Some(global_aliases) = cp.aliases_by_provider.get("*") {
                ids.extend(
                    global_aliases
                        .iter()
                        .filter(|alias| target_permitted(&cp, identity, &alias.target))
                        .map(|alias| alias.alias.clone()),
                );
            }
            for (provider, mut provider_models) in providers.iter().zip(catalogues) {
                if let Some(models) = cp.exposed_models_by_provider.get(&provider.id) {
                    provider_models.extend(local_ops::entries_from(models));
                }
                ids.extend(provider_models.into_iter().filter_map(|model| {
                    authz::provider_model_permitted(&cp, identity, &provider.name, &model.id)
                        .then(|| format!("{}/{}", provider.name, model.id))
                }));
                if let Some(aliases) = cp.aliases_by_provider.get(&provider.name) {
                    ids.extend(
                        aliases
                            .iter()
                            .filter(|alias| {
                                let target = preprocess::apply_provider_alias(
                                    &cp,
                                    &provider.name,
                                    &alias.alias,
                                );
                                authz::provider_model_permitted(
                                    &cp,
                                    identity,
                                    &provider.name,
                                    &target,
                                )
                            })
                            .map(|alias| format!("{}/{}", provider.name, alias.alias)),
                    );
                }
            }
            ids.sort();
            ids.dedup();
            let entries: Vec<ModelEntry> = ids
                .into_iter()
                .map(|id| ModelEntry {
                    id,
                    display_name: None,
                })
                .collect();
            local_ops::render_model_list(family, &entries)
        }
        _ => {
            let cp = state.cp();
            let id = classify::path_model_id(&ctx.path).ok_or(PipelineError::UnsupportedPath)?;
            if !resolved_target_permitted(&cp, identity, &id) {
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

fn target_permitted(
    cp: &crate::app::snapshot::ControlPlaneSnapshot,
    identity: &crate::app::snapshot::KeyIdentity,
    target: &str,
) -> bool {
    if cp.routes_by_name.contains_key(target) {
        return authz::permitted(cp, identity, target);
    }

    preprocess::split_provider_model(target)
        .and_then(|(provider_name, model)| {
            let provider = cp
                .providers_by_name
                .get(provider_name)
                .filter(|provider| provider.enabled)?;
            let model = preprocess::apply_provider_alias(cp, provider_name, model);
            Some(authz::provider_model_permitted(
                cp,
                identity,
                &provider.name,
                &model,
            ))
        })
        .unwrap_or(false)
}

fn resolved_target_permitted(
    cp: &crate::app::snapshot::ControlPlaneSnapshot,
    identity: &crate::app::snapshot::KeyIdentity,
    model: &str,
) -> bool {
    let target = preprocess::apply_global_alias(cp, model);
    target_permitted(cp, identity, &target)
}

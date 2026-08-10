//! Snapshot-owned request admission and candidate resolution.
//!
//! Parsing and control-plane lookups are synchronous. The resulting plans own
//! exactly the provider, credential, authz, pricing, and transform data needed
//! by cache-backed admission and balancing.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::app::AppState;
use crate::app::snapshot::{ControlPlaneSnapshot, KeyIdentity};
use crate::billing::pending;
use crate::pipeline::context::{Candidate, RequestCtx, RoutingMode};
use crate::pipeline::error::PipelineError;
use crate::pipeline::outcome::ExecOutcome;
use crate::pipeline::{authz, balance, model_catalog, preprocess, route, transform};
use crate::protocol::{Operation, OperationKey};
use crate::store::persistence::records::{Provider, Scope};
use crate::util::time::unix_now;

type CandidateKey = (i64, String);

enum CandidateSource {
    Route(balance::PreparedRoute),
    Provider(balance::PreparedProvider),
}

struct AffinityInput {
    session_id: Option<String>,
    conversation_fingerprint: Option<[u8; 32]>,
}

pub(crate) struct CandidateRequest {
    authorization: authz::AuthorizationPlan,
    quota: authz::QuotaPlan,
    source: CandidateSource,
    estimates: HashMap<CandidateKey, i64>,
    synthetic_providers: HashSet<i64>,
    route_name: Option<String>,
    provider_name: Option<String>,
    affinity: AffinityInput,
    credential_binding_key: Option<Arc<str>>,
}

pub(crate) struct ScopedModels {
    authorization: authz::AuthorizationPlan,
    provider: Arc<Provider>,
    identity: Arc<KeyIdentity>,
    source: OperationKey,
}

pub(crate) enum Prepared {
    Candidates(Box<CandidateRequest>),
    ScopedModels(ScopedModels),
}

pub(crate) struct Admitted {
    pub candidates: Vec<Candidate>,
    pub est_micros: i64,
    pub quota_scopes: Vec<(Scope, i64)>,
    pub synthesize_stream: bool,
}

pub(crate) fn prepare(
    cp: &ControlPlaneSnapshot,
    ctx: &RequestCtx,
    op: OperationKey,
    affinity_session_id: Option<String>,
    conversation_fingerprint: Option<[u8; 32]>,
) -> Result<Prepared, PipelineError> {
    let identity = ctx.identity.as_ref().expect("auth ran first");
    let affinity = AffinityInput {
        session_id: affinity_session_id,
        conversation_fingerprint,
    };
    match &ctx.mode {
        RoutingMode::Aggregated => prepare_aggregated(cp, ctx, identity, affinity),
        RoutingMode::Namespace { namespace } => {
            prepare_namespace(cp, ctx, identity, namespace, affinity)
        }
        RoutingMode::Scoped { provider } => {
            prepare_scoped(cp, ctx, op, identity, provider, affinity)
        }
        RoutingMode::Named { .. } => unreachable!("named routing mode must be resolved first"),
    }
}

fn prepare_aggregated(
    cp: &ControlPlaneSnapshot,
    ctx: &RequestCtx,
    identity: &Arc<KeyIdentity>,
    affinity: AffinityInput,
) -> Result<Prepared, PipelineError> {
    let model = preprocess::preprocess(cp, ctx)?;
    if cp.routes_by_name.contains_key(&model) {
        let resolved = route::route(cp, &model)?;
        let authorization = authz::prepare(cp, identity, &model)?;
        let source = CandidateSource::Route(balance::prepare(cp, resolved));
        return Ok(Prepared::Candidates(Box::new(prepare_request(
            cp,
            ctx,
            authorization,
            source,
            Some(model),
            None,
            affinity,
        ))));
    }

    let Some((provider_name, requested)) = preprocess::split_provider_model(&model) else {
        return Err(PipelineError::UnknownRoute(model));
    };
    let provider = enabled_provider(cp, provider_name)?;
    let authorized_model = preprocess::apply_provider_alias(cp, &provider.name, requested);
    let authorization =
        authz::prepare_provider_model(cp, identity, &provider.name, &authorized_model)?;
    let source = CandidateSource::Provider(prepare_provider(cp, &provider, requested));
    Ok(Prepared::Candidates(Box::new(prepare_request(
        cp,
        ctx,
        authorization,
        source,
        None,
        Some(provider.name.clone()),
        affinity,
    ))))
}

fn prepare_namespace(
    cp: &ControlPlaneSnapshot,
    ctx: &RequestCtx,
    identity: &Arc<KeyIdentity>,
    namespace: &str,
    affinity: AffinityInput,
) -> Result<Prepared, PipelineError> {
    let model = preprocess::preprocess(cp, ctx)?;
    let resolved = route::route_in_namespace(cp, namespace, &model)?;
    let authorization = authz::prepare(cp, identity, &model)?;
    let source = CandidateSource::Route(balance::prepare(cp, resolved));
    Ok(Prepared::Candidates(Box::new(prepare_request(
        cp,
        ctx,
        authorization,
        source,
        Some(model),
        None,
        affinity,
    ))))
}

fn prepare_scoped(
    cp: &ControlPlaneSnapshot,
    ctx: &RequestCtx,
    op: OperationKey,
    identity: &Arc<KeyIdentity>,
    provider_name: &str,
    affinity: AffinityInput,
) -> Result<Prepared, PipelineError> {
    let provider = enabled_provider(cp, provider_name)?;
    if op.operation() == Operation::ListModels {
        return Ok(Prepared::ScopedModels(ScopedModels {
            authorization: authz::prepare_provider_listing(cp, identity, &provider.name)?,
            provider,
            identity: Arc::clone(identity),
            source: op,
        }));
    }

    let requested = preprocess::requested_model(ctx);
    let authorization = match &requested {
        Some(requested) => {
            let model = preprocess::apply_provider_alias(cp, &provider.name, requested);
            authz::prepare_provider_model(cp, identity, &provider.name, &model)?
        }
        None => authz::prepare(cp, identity, &provider.name)?,
    };
    let requested = requested.unwrap_or_default();
    let source = CandidateSource::Provider(prepare_provider(cp, &provider, &requested));
    Ok(Prepared::Candidates(Box::new(prepare_request(
        cp,
        ctx,
        authorization,
        source,
        None,
        Some(provider.name.clone()),
        affinity,
    ))))
}

fn enabled_provider(cp: &ControlPlaneSnapshot, name: &str) -> Result<Arc<Provider>, PipelineError> {
    cp.providers_by_name
        .get(name)
        .filter(|provider| provider.enabled)
        .cloned()
        .ok_or_else(|| PipelineError::UnknownProvider(name.to_owned()))
}

fn prepare_provider(
    cp: &ControlPlaneSnapshot,
    provider: &Arc<Provider>,
    requested: &str,
) -> balance::PreparedProvider {
    let requested = preprocess::apply_provider_alias(cp, &provider.name, requested);
    balance::prepare_provider(cp, provider, requested)
}

fn prepare_request(
    cp: &ControlPlaneSnapshot,
    ctx: &RequestCtx,
    authorization: authz::AuthorizationPlan,
    source: CandidateSource,
    route_name: Option<String>,
    provider_name: Option<String>,
    affinity: AffinityInput,
) -> CandidateRequest {
    let op = ctx.op.expect("classified");
    let identity = ctx.identity.as_deref().expect("auth ran first");
    let pairs: Vec<(i64, &str)> = match &source {
        CandidateSource::Route(route) => route.provider_models().collect(),
        CandidateSource::Provider(provider) => vec![provider.provider_model()],
    };
    let estimates = pairs
        .iter()
        .map(|(provider_id, model)| {
            (
                (*provider_id, (*model).to_owned()),
                estimate(cp, ctx, *provider_id, model),
            )
        })
        .collect();
    let synthetic_providers = pairs
        .iter()
        .filter_map(|(provider_id, _)| {
            matches!(
                transform::plan_for(cp, *provider_id, op),
                Ok(transform::TransformPlan::SynthesizeStream { .. })
            )
            .then_some(*provider_id)
        })
        .collect();
    CandidateRequest {
        authorization,
        quota: authz::prepare_quota(cp, identity),
        source,
        estimates,
        synthetic_providers,
        route_name,
        provider_name,
        affinity,
        credential_binding_key: balance::credential_binding_key(ctx, identity.user_key.id),
    }
}

impl CandidateRequest {
    pub(crate) fn route_name(&self) -> Option<&str> {
        self.route_name.as_deref()
    }

    pub(crate) fn provider_name(&self) -> Option<&str> {
        self.provider_name.as_deref()
    }

    pub(crate) async fn admit(
        self,
        state: &AppState,
        identity: &KeyIdentity,
        stream: bool,
    ) -> Result<Admitted, PipelineError> {
        let now = unix_now();
        authz::authorize(&self.authorization, state.cache.as_ref(), now).await?;
        let bound_credential = balance::read_credential_binding(
            state.cache.as_ref(),
            self.credential_binding_key.as_deref(),
        )
        .await;
        let mut candidates = match &self.source {
            CandidateSource::Route(route) => {
                balance::candidates(
                    route,
                    state.health.as_ref(),
                    state.cache.as_ref(),
                    Some(identity.user_key.id),
                    self.affinity.session_id.as_deref(),
                    bound_credential,
                    self.affinity.conversation_fingerprint.as_ref(),
                )
                .await?
            }
            CandidateSource::Provider(provider) => {
                provider
                    .candidates(
                        state.health.as_ref(),
                        state.cache.as_ref(),
                        Some(identity.user_key.id),
                        bound_credential,
                    )
                    .await?
            }
        };
        for candidate in &mut candidates {
            candidate.credential_binding_key = self.credential_binding_key.clone();
        }
        let est_micros = candidates
            .iter()
            .filter_map(|candidate| {
                self.estimates
                    .get(&(candidate.provider.id, candidate.upstream_model_id.clone()))
            })
            .copied()
            .max()
            .unwrap_or(0);
        authz::precheck_quota(&self.quota, state.cache.as_ref(), est_micros, now).await?;
        let quota_scopes = if est_micros > 0 {
            authz::prepared_quota_scopes(&self.quota)
        } else {
            Vec::new()
        };
        let synthesize_stream = stream
            && candidates
                .iter()
                .any(|candidate| self.synthetic_providers.contains(&candidate.provider.id));
        Ok(Admitted {
            candidates,
            est_micros,
            quota_scopes,
            synthesize_stream,
        })
    }
}

impl ScopedModels {
    pub(crate) fn provider_name(&self) -> &str {
        &self.provider.name
    }

    pub(crate) async fn serve(self, state: &AppState) -> Result<ExecOutcome, PipelineError> {
        authz::authorize(&self.authorization, state.cache.as_ref(), unix_now()).await?;
        Ok(model_catalog::serve_scoped(state, self.provider, self.identity, self.source).await)
    }
}

/// §17 pre-deduct estimate in micro-dollars for billable operations.
fn estimate(cp: &ControlPlaneSnapshot, ctx: &RequestCtx, provider_id: i64, model_id: &str) -> i64 {
    let Some(op) = ctx.op else { return 0 };
    if matches!(
        transform::plan_for(cp, provider_id, op),
        Ok(transform::TransformPlan::Local)
    ) {
        return 0;
    }
    match op.operation() {
        Operation::GenerateContent
        | Operation::StreamGenerateContent
        | Operation::CompactContent
        | Operation::CreateEmbedding
        | Operation::Rerank
        | Operation::CreateImage
        | Operation::EditImage => {
            let pricing = pending::model_pricing(cp, provider_id, model_id);
            pending::estimate_micros(&pricing, ctx.body.len())
        }
        _ => 0,
    }
}

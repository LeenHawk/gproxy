//! The handle's half of `/admin/api`: the configuration families, through
//! [`Gproxy::manage`](gproxy_sdk::Gproxy::manage) — except the two whose rows
//! carry an owner, which go through
//! [`ScopedManage`](gproxy_app::ScopedManage).
//!
//! The surface's rules — one explicit route per operation, thin handlers, the
//! middleware order, how a section declares which scopes reach it, and why the
//! audit action is derived rather than typed — are documented once on the
//! parent module, which also owns the macros used here.
//!
//! # Two families are scope-aware; the rest is instance machinery
//!
//! `credentials` and `quotas` are what a tenant actually owns: the upstream
//! accounts an organization pays for, and the budgets it sets on them. Both
//! carry an owner, both are declared `Scoped`, and both are reached through
//! `scoped_family!` so the scope participates in building the query.
//!
//! Everything else configures the **gateway** — providers, models, routes,
//! rewrite rules, endpoints, prices, profiles, settings, transfer,
//! connectivity, the tokenizer and the static catalogues — and is declared
//! `Instance`. An organization administrator does not run the upstreams.
//!
//! The `quotas` table holds both halves of that split at once: a row owned by
//! `org` or `team` is a tenant budget, and one owned by `credential` or
//! `provider` is an operator limit. The split is by `owner_kind`, not by
//! route, so `/quotas/{id}/limit-reset` is routed like the rest and simply
//! answers `NotFound` outside the instance scope — the row it names is never
//! inside an organization.
//!
//! # Paths
//!
//! Where v3 had the same operation, the path is v3's: `/providers`,
//! `/credentials/{id}/quota-probe`, `/rule-sets/{id}/rule-presets/{preset}`,
//! `/export`, `/tokenizer-vocabs` and the rest. An operator's muscle memory
//! and whatever scripts a deployment has are worth more than a tidier noun.
//! The families v4 gained — connection profiles, operation
//! rules and endpoints, price tiers — follow the plural-noun convention the
//! rest of the table already uses.
//!
//! # Two routes that are not like the others
//!
//! `POST /export` can carry sealed credential blobs, so its answer is as
//! sensitive as the database file it came from: it is sent `no-store` so that
//! no proxy and no browser keeps a copy of it on disk.
//!
//! `POST /credentials/{id}/reveal` is a **read** that is deliberately spelled
//! as a write, to retain the existing disclosure contract. Both reads and writes are audited. See the
//! handler.

use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use gproxy_app::{AdminScope, AppError, ScopedManage};
use gproxy_sdk::{
    BudgetOwner, CredentialStatus, RefreshMode,
    dto::{
        ApplyDefaultPricesRequest, ApplyRulePreset, BatchItem, ConnectionProfilePatch,
        ConnectionProfileWrite, ConnectivityTest, CredentialPatch, CredentialWrite, ExportRequest,
        ImportRequest, ListQuery, ModelPatch, ModelTest, ModelWrite, OperationEndpointPatch,
        OperationEndpointWrite, OperationRulePatch, OperationRuleWrite, PriceRatePatch,
        PriceRateWrite, PriceRulePatch, PriceRuleWrite, PriceTierPatch, PriceTierWrite,
        ProviderModelPatch, ProviderModelWrite, ProviderPatch, ProviderRuleSetPatch,
        ProviderRuleSetWrite, ProviderWrite, QuotaObservationQuery, QuotaPatch, QuotaWrite,
        RewriteRulePatch, RewriteRuleWrite, RouteMemberPatch, RouteMemberWrite, RoutePatch,
        RouteWrite, RuleSetPatch, RuleSetWrite, SettingsPatch, TokenizerFetch,
    },
};
use gproxy_seaorm::BatchConnectionTrait;
use http::{HeaderValue, header};
use serde::Deserialize;

use super::{reply, reply_empty, reply_sdk, reply_sdk_empty};
use crate::{HostState, error::ErrorResponse};

/// The configuration routes, to be merged into `/admin/api` and guarded there.
pub(super) fn routes<C>() -> Router<HostState<C>>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let router = config_family!(
        Router::new(),
        "/providers",
        [providers()],
        ProviderWrite,
        ProviderPatch,
        "providers"
    )
    .route(
        "/providers/{id}/routing-defaults/reset",
        post(reset_routing_defaults::<C>),
    );

    let router = scoped_family!(
        router,
        "/credentials",
        credentials,
        CredentialWrite,
        CredentialPatch,
        "credentials"
    )
    .route("/credentials/{id}/reveal", post(reveal_secret::<C>))
    .route("/credentials/{id}/status", post(set_status::<C>))
    .route("/credentials/{id}/refresh", post(refresh::<C>))
    .route("/credentials/{id}/quota", get(quota_read::<C>))
    .route(
        "/credentials/{id}/quota-observations",
        get(quota_observations::<C>),
    )
    .route("/credentials/{id}/quota-probe", post(quota_probe::<C>))
    .route(
        "/credentials/{id}/quota-diagnostics",
        post(quota_diagnostics::<C>),
    )
    .route(
        "/credentials/{id}/quota-reset-credits",
        get(quota_reset_credits::<C>),
    )
    .route("/credentials/{id}/quota-reset", post(quota_reset::<C>))
    .route("/credentials/{id}/health-reset", post(health_reset::<C>))
    // `Quotas::limit_status` is the same call on the same argument; a
    // credential is what either of them is keyed by, so it is routed once.
    .route("/credentials/{id}/limits", get(limit_status::<C>));

    // `/models/{discover,test}` are v3's paths and stay static segments in
    // front of `/models/{id}`: axum prefers the literal, so a model may not be
    // addressed by either name — which is exactly what v3 did too.
    let router = config_family!(
        router,
        "/models",
        [models()],
        ModelWrite,
        ModelPatch,
        "models"
    )
    .route("/models/discover", post(discover_models::<C>))
    .route("/models/discover/apply", post(apply_discovered::<C>))
    .route("/models/test", post(model_test::<C>));

    let router = config_family!(
        router,
        "/provider-models",
        [provider_models()],
        ProviderModelWrite,
        ProviderModelPatch,
        "provider-models"
    );

    let router = config_family!(
        router,
        "/routes",
        [routes()],
        RouteWrite,
        RoutePatch,
        "routes"
    );
    let router = config_family!(
        router,
        "/route-members",
        [route_members()],
        RouteMemberWrite,
        RouteMemberPatch,
        "route-members"
    );
    let router = config_family!(
        router,
        "/connection-profiles",
        [connection_profiles()],
        ConnectionProfileWrite,
        ConnectionProfilePatch,
        "connection-profiles"
    );

    // One durable row with two groups in it, so one route rather than v3's
    // `/instance-settings` and `/log-settings`.
    let router = router.route(
        "/settings",
        get(read_settings::<C>).patch(patch_settings::<C>),
    );

    let router = config_family!(
        router,
        "/rule-sets",
        [rewrite().sets()],
        RuleSetWrite,
        RuleSetPatch,
        "rule-sets"
    )
    .route(
        "/providers/{id}/default-rule-set",
        post(default_rule_set::<C>),
    )
    .route("/rule-sets/{id}/rules", put(replace_rules::<C>))
    .route(
        "/rule-sets/{id}/rule-presets/{preset}",
        post(apply_rule_preset::<C>),
    );
    let router = config_family!(
        router,
        "/rules",
        [rewrite().rules()],
        RewriteRuleWrite,
        RewriteRulePatch,
        "rules"
    );
    let router = config_family!(
        router,
        "/provider-rule-sets",
        [rewrite().bindings()],
        ProviderRuleSetWrite,
        ProviderRuleSetPatch,
        "provider-rule-sets"
    );

    let router = config_family!(
        router,
        "/operation-rules",
        [endpoints().operation_rules()],
        OperationRuleWrite,
        OperationRulePatch,
        "operation-rules"
    )
    .route("/providers/{id}/routing", get(provider_routing::<C>))
    .route(
        "/providers/{id}/routing/{operation}/{dialect}",
        put(set_provider_routing::<C>).delete(reset_provider_routing::<C>),
    );
    let router = config_family!(
        router,
        "/operation-endpoints",
        [endpoints().operation_endpoints()],
        OperationEndpointWrite,
        OperationEndpointPatch,
        "operation-endpoints"
    );

    let router = scoped_family!(router, "/quotas", quotas, QuotaWrite, QuotaPatch, "quotas")
        .route("/quotas/status", get(budget_status::<C>))
        .route("/quotas/{id}/reset", post(reset_budget::<C>))
        .route("/quotas/{id}/limit-reset", post(reset_limit::<C>));

    let router = config_family!(
        router,
        "/price-rules",
        [pricing().rules()],
        PriceRuleWrite,
        PriceRulePatch,
        "price-rules"
    );
    let router = config_family!(
        router,
        "/price-rates",
        [pricing().rates()],
        PriceRateWrite,
        PriceRatePatch,
        "price-rates"
    );
    let router = config_family!(
        router,
        "/price-tiers",
        [pricing().tiers()],
        PriceTierWrite,
        PriceTierPatch,
        "price-tiers"
    );

    router
        .route("/export", post(export::<C>))
        .route("/import", post(import::<C>))
        .route("/connectivity/test", post(connectivity_test::<C>))
        .route("/channels", get(channels::<C>))
        .route("/tls-presets", get(tls_presets::<C>))
        .route("/rule-presets", get(rule_presets::<C>))
        .route("/default-model-catalog", get(default_models::<C>))
        .route("/models/openrouter", get(openrouter_models::<C>))
        .route(
            "/default-model-catalog/apply-prices",
            post(apply_default_prices::<C>),
        )
        .route(
            "/tokenizer-vocabs",
            get(vocabularies::<C>).post(fetch_vocabulary::<C>),
        )
        .route("/tokenizer-vocabs/progress", get(fetch_progress::<C>))
        .route(
            "/tokenizer-vocabs/{id}",
            axum::routing::delete(delete_vocabulary::<C>),
        )
        .route(
            "/tokenizer-auth",
            get(tokenizer_auth::<C>).patch(set_tokenizer_auth::<C>),
        )
        .route("/tokenizer-auth/reveal", post(reveal_tokenizer_auth::<C>))
}

// ------------------------------------------------------------- providers --

async fn reset_routing_defaults<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path(id): Path<String>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        gate!("providers", scope);
        // A row count, not a row: the answer is how much was dropped.
        match state
            .app()
            .gproxy()
            .manage()
            .providers()
            .reset_routing_defaults(&id)
            .await
        {
            Ok(cleared) => crate::error::ok_json(&serde_json::json!({ "cleared": cleared })),
            Err(error) => ErrorResponse(AppError::from(error)).into_response(),
        }
    })
    .await
}

// ----------------------------------------------------------- credentials --

/// The one read on this surface that is audited.
///
/// It is a `POST` so that it is: the audit middleware deliberately skips safe
/// methods, because a trail that records every list is a trail nobody reads.
/// Disclosing a secret is the exception worth keeping — so it is spelled as an
/// unsafe method and lands in `audit_event` as `admin.credentials.reveal`.
async fn reveal_secret<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path(id): Path<String>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { scoped!(state, scope, "credentials", credentials.reveal_secret(&id)) })
        .await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StatusBody {
    status: CredentialStatus,
    #[serde(default)]
    reason: Option<String>,
}

async fn set_status<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path(id): Path<String>,
    Json(body): Json<StatusBody>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        scoped!(
            state,
            scope,
            "credentials",
            credentials.set_status(&id, body.status, body.reason)
        )
    })
    .await
}

/// `?force=true` renews material that has not expired yet, which is what an
/// operator testing a channel's refresh actually wants.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ForceQuery {
    #[serde(default)]
    force: bool,
}

impl ForceQuery {
    fn mode(&self) -> RefreshMode {
        if self.force {
            RefreshMode::Force
        } else {
            RefreshMode::IfNeeded
        }
    }
}

async fn refresh<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path(id): Path<String>,
    Query(query): Query<ForceQuery>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        scoped!(
            state,
            scope,
            "credentials",
            credentials.refresh(&id, query.mode())
        )
    })
    .await
}

async fn quota_read<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path(id): Path<String>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { scoped!(state, scope, "credentials", credentials.quota_read(&id)) })
        .await
}

async fn quota_observations<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path(id): Path<String>,
    Query(query): Query<QuotaObservationQuery>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        scoped!(
            state,
            scope,
            "credentials",
            credentials.quota_observations(&id, query)
        )
    })
    .await
}

async fn quota_probe<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path(id): Path<String>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { scoped!(state, scope, "credentials", credentials.quota_probe(&id)) })
        .await
}

async fn quota_diagnostics<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path(id): Path<String>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        let mut response = scoped!(
            state,
            scope,
            "credentials",
            credentials.quota_diagnostics(&id)
        );
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        response
    })
    .await
}

async fn quota_reset_credits<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path(id): Path<String>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        scoped!(
            state,
            scope,
            "credentials",
            credentials.quota_reset_credits(&id)
        )
    })
    .await
}

async fn quota_reset<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path(id): Path<String>,
    body: Option<Json<gproxy_sdk::dto::QuotaResetWrite>>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        scoped!(
            state,
            scope,
            "credentials",
            credentials.quota_reset_with(&id, body.map(|Json(body)| body).unwrap_or_default())
        )
    })
    .await
}

async fn health_reset<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path(id): Path<String>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { scoped!(state, scope, "credentials", credentials.health_reset(&id)) })
        .await
}

async fn limit_status<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path(id): Path<String>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { scoped!(state, scope, "credentials", credentials.limit_status(&id)) })
        .await
}

// ---------------------------------------------------------------- models --

/// The two arguments `discover_models` takes. It is a request shape rather
/// than a DTO because the sdk's own signature is two arguments — there is
/// nothing to mirror.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiscoverBody {
    provider_id: String,
    #[serde(default)]
    credential_id: Option<String>,
}

async fn discover_models<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Json(body): Json<DiscoverBody>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        manage!(
            state,
            scope,
            "models",
            connectivity().discover_models(&body.provider_id, body.credential_id.as_deref())
        )
    })
    .await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApplyDiscoveredBody {
    provider_id: String,
    #[serde(default)]
    upstream_names: Vec<String>,
}

async fn apply_discovered<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Json(body): Json<ApplyDiscoveredBody>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        manage!(
            state,
            scope,
            "models",
            connectivity().apply_discovered(&body.provider_id, body.upstream_names)
        )
    })
    .await
}

async fn model_test<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Json(request): Json<ModelTest>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { manage!(state, scope, "models", connectivity().model_test(request)) })
        .await
}

// -------------------------------------------------------------- settings --

async fn read_settings<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { manage!(state, scope, "settings", settings().get()) }).await
}

async fn patch_settings<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Json(patch): Json<SettingsPatch>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        gate!("settings", scope);
        match state.app().gproxy().manage().settings().update(patch).await {
            Ok(settings) => match state.app().reload_all().await {
                Ok(_) => crate::error::ok_json(&settings),
                Err(error) => ErrorResponse(error).into_response(),
            },
            Err(error) => reply_sdk::<gproxy_sdk::dto::SettingsDto>(Err(error)),
        }
    })
    .await
}

// --------------------------------------------------------------- rewrite --

/// The whole set at once, which is why it is a `PUT`: an editor saves a list,
/// and a half-applied reorder is a rule set nobody wrote.
async fn default_rule_set<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path(id): Path<String>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        gate!("providers", scope);
        gate!("provider-rule-sets", scope);
        manage!(state, scope, "rule-sets", rewrite().ensure_default_set(&id))
    })
    .await
}

async fn replace_rules<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path(id): Path<String>,
    Json(rules): Json<Vec<RewriteRuleWrite>>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        manage!(
            state,
            scope,
            "rule-sets",
            rewrite().replace_rules(&id, rules)
        )
    })
    .await
}

/// Both ids come from the path, so a body cannot point the write at a
/// different rule set than the one the caller addressed.
async fn apply_rule_preset<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path((rule_set_id, preset_id)): Path<(String, String)>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        manage!(
            state,
            scope,
            "rule-sets",
            catalog().apply_rule_preset(ApplyRulePreset {
                rule_set_id,
                preset_id,
            })
        )
    })
    .await
}

// ---------------------------------------------------------------- quotas --

/// `?owners=user:alice,team:t1`.
///
/// A `GET` with a repeated-pair query rather than a `POST` with a list body,
/// because this is what a console polls while it renders a budget bar — and
/// every `POST` on this surface writes an audit row.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OwnersQuery {
    #[serde(default)]
    owners: String,
}

async fn budget_status<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Query(query): Query<OwnersQuery>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        let mut owners = Vec::new();
        for pair in query
            .owners
            .split(',')
            .map(str::trim)
            .filter(|pair| !pair.is_empty())
        {
            // Only the first colon splits: an owner kind never contains one, and
            // an id supplied by a host might.
            match pair.split_once(':') {
                Some((kind, id)) if !kind.is_empty() && !id.is_empty() => {
                    owners.push(BudgetOwner::new(kind, id));
                }
                _ => {
                    return ErrorResponse(AppError::invalid(format!(
                        "owner `{pair}` is not in `kind:id` form"
                    )))
                    .into_response();
                }
            }
        }
        scoped!(state, scope, "quotas", quotas.budget_status(&owners))
    })
    .await
}

async fn reset_budget<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path(id): Path<String>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { scoped!(state, scope, "quotas", quotas.reset_budget(&id)) }).await
}

/// The other half of the same table: a `credential` or `provider` row is an
/// operator limit rather than a budget, and resetting it unblocks credentials
/// rather than reopening a window. No organization scope owns such a row, so
/// outside the instance scope this is `NotFound` by the same rule as any other
/// row.
async fn reset_limit<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path(id): Path<String>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { scoped!(state, scope, "quotas", quotas.reset_limit(&id)) }).await
}

// -------------------------------------------------------------- transfer --

/// An export may carry sealed credential blobs, so it is exactly as sensitive
/// as the database file it came from. `no-store` keeps it out of a proxy's and
/// a browser's disk cache; the authorization is the surface's own.
async fn export<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Json(request): Json<ExportRequest>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        gate!("transfer", scope);
        let mut response = reply_sdk(
            state
                .app()
                .gproxy()
                .manage()
                .transfer()
                .export(request)
                .await,
        );
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        response
    })
    .await
}

async fn import<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Json(request): Json<ImportRequest>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        gate!("transfer", scope);
        match state.app().gproxy().manage().transfer().import(request).await {
            Ok(mut report) => {
                if state.app().reload_all().await.is_err() {
                    report.warnings.push("Configuration was imported, but runtime reload failed. Retry reload or restart the instance.".to_owned());
                }
                crate::error::ok_json(&report)
            }
            Err(error) => reply_sdk::<gproxy_sdk::dto::ImportReportDto>(Err(error)),
        }
    }).await
}

// ---------------------------------------------------------- connectivity --

async fn connectivity_test<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Json(request): Json<ConnectivityTest>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { manage!(state, scope, "connectivity", connectivity().test(request)) })
        .await
}

// --------------------------------------------------------------- catalog --

/// Every compiled-in channel as data, which is what a console renders a
/// provider form from. `ChannelDescriptor` goes over the wire as itself.
async fn channels<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        gate!("catalog", scope);
        crate::error::ok_json(&state.app().gproxy().manage().catalog().channels())
    })
    .await
}

async fn tls_presets<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        gate!("catalog", scope);
        crate::error::ok_json(&state.app().gproxy().manage().catalog().tls_presets())
    })
    .await
}

async fn rule_presets<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        gate!("catalog", scope);
        crate::error::ok_json(&state.app().gproxy().manage().catalog().rule_presets())
    })
    .await
}

/// The prices and context windows this release was built with. It is parsed
/// from a compiled-in asset rather than read, so there is nothing to await.
async fn default_models<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        gate!("catalog", scope);
        reply_sdk(state.app().gproxy().manage().catalog().default_models())
    })
    .await
}

async fn openrouter_models<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        gate!("models", scope);
        reply_sdk(
            state
                .app()
                .gproxy()
                .manage()
                .connectivity()
                .openrouter_models()
                .await,
        )
    })
    .await
}

async fn apply_default_prices<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Json(request): Json<ApplyDefaultPricesRequest>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        manage!(
            state,
            scope,
            "catalog",
            catalog().apply_default_prices(request)
        )
    })
    .await
}

// ------------------------------------------------------------- tokenizer --

async fn vocabularies<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { manage!(state, scope, "tokenizer", tokenizer().vocabularies()) }).await
}

async fn fetch_vocabulary<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Json(request): Json<TokenizerFetch>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { manage!(state, scope, "tokenizer", tokenizer().fetch(request)) }).await
}

/// How far the download running **in this process** has got. A peer's is
/// invisible here, which is why it is progress rather than state.
async fn fetch_progress<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        gate!("tokenizer", scope);
        crate::error::ok_json(&state.app().gproxy().manage().tokenizer().progress())
    })
    .await
}

async fn delete_vocabulary<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path(id): Path<String>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { manage!(@empty state, scope, "tokenizer", tokenizer().delete(&id)) })
        .await
}

async fn tokenizer_auth<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { manage!(state, scope, "tokenizer", tokenizer().auth()) }).await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TokenBody {
    /// `null` clears the token and downloads anonymously again.
    #[serde(default)]
    token: Option<String>,
}

async fn set_tokenizer_auth<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Json(body): Json<TokenBody>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { manage!(state, scope, "tokenizer", tokenizer().set_auth(body.token)) })
        .await
}

/// The other deliberate disclosure, for the same reason as `reveal_secret`:
/// a `POST`, so the trail records it.
async fn reveal_tokenizer_auth<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        gate!("tokenizer", scope);
        match state
            .app()
            .gproxy()
            .manage()
            .tokenizer()
            .reveal_auth()
            .await
        {
            Ok(token) => crate::error::ok_json(&serde_json::json!({ "token": token })),
            Err(error) => ErrorResponse(AppError::from(error)).into_response(),
        }
    })
    .await
}

async fn provider_routing<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path(id): Path<String>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        manage!(
            state,
            scope,
            "operation-rules",
            endpoints().operation_rules().effective(&id)
        )
    })
    .await
}

async fn set_provider_routing<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path((id, operation, dialect)): Path<(String, String, String)>,
    Json(write): Json<gproxy_sdk::dto::RoutingMappingWrite>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        manage!(
            state,
            scope,
            "operation-rules",
            endpoints()
                .operation_rules()
                .set_mapping(&id, &operation, &dialect, write)
        )
    })
    .await
}
async fn reset_provider_routing<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path((id, operation, dialect)): Path<(String, String, String)>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        manage!(
            state,
            scope,
            "operation-rules",
            endpoints()
                .operation_rules()
                .reset_mapping(&id, &operation, &dialect)
        )
    })
    .await
}

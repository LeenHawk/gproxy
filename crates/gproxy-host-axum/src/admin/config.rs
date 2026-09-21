//! The handle's half of `/admin/api`: the configuration families, through
//! [`Gproxy::manage`](gproxy_sdk::Gproxy::manage).
//!
//! The surface's rules — one explicit route per operation, thin handlers, the
//! middleware order and why the audit action is derived rather than typed —
//! are documented once on the parent module, which also owns the macros used
//! here.
//!
//! # Paths
//!
//! Where v3 had the same operation, the path is v3's: `/providers`,
//! `/credentials/{id}/quota-probe`, `/rule-sets/{id}/rule-presets/{preset}`,
//! `/export`, `/tokenizer-vocabs` and the rest. An operator's muscle memory
//! and whatever scripts a deployment has are worth more than a tidier noun.
//! The families v4 gained — exposed models, connection profiles, operation
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
//! as a write, because the audit middleware skips reads and this is the one
//! disclosure an operator must be able to account for afterwards. See the
//! handler.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use gproxy_app::AppError;
use gproxy_sdk::{
    BudgetOwner, CredentialStatus, RefreshMode,
    dto::{
        ApplyDefaultPricesRequest, ApplyRulePreset, BatchItem, ConnectionProfilePatch,
        ConnectionProfileWrite, ConnectivityTest, CredentialPatch, CredentialWrite, ExportRequest,
        ExposedModelPatch, ExposedModelWrite, ImportRequest, ListQuery, ModelPatch, ModelTest,
        ModelWrite, OperationEndpointPatch, OperationEndpointWrite, OperationRulePatch,
        OperationRuleWrite, PriceRatePatch, PriceRateWrite, PriceRulePatch, PriceRuleWrite,
        PriceTierPatch, PriceTierWrite, ProviderModelPatch, ProviderModelWrite, ProviderPatch,
        ProviderRuleSetPatch, ProviderRuleSetWrite, ProviderWrite, QuotaPatch, QuotaWrite,
        RewriteRulePatch, RewriteRuleWrite, RouteMemberPatch, RouteMemberWrite, RoutePatch,
        RouteWrite, RuleSetPatch, RuleSetWrite, SettingsPatch, TokenizerFetch,
    },
};
use gproxy_seaorm::BatchConnectionTrait;
use http::{HeaderValue, header};
use serde::Deserialize;

use super::{reply_sdk, reply_sdk_empty};
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
        ProviderPatch
    )
    .route(
        "/providers/{id}/routing-defaults/reset",
        post(reset_routing_defaults::<C>),
    );

    let router = config_family!(
        router,
        "/credentials",
        [credentials()],
        CredentialWrite,
        CredentialPatch
    )
    .route("/credentials/{id}/reveal", post(reveal_secret::<C>))
    .route("/credentials/{id}/status", post(set_status::<C>))
    .route("/credentials/{id}/refresh", post(refresh::<C>))
    .route("/credentials/{id}/quota", get(quota_read::<C>))
    .route("/credentials/{id}/quota-probe", post(quota_probe::<C>))
    .route("/credentials/{id}/quota-reset", post(quota_reset::<C>))
    .route("/credentials/{id}/health-reset", post(health_reset::<C>))
    // `Quotas::limit_status` is the same call on the same argument; a
    // credential is what either of them is keyed by, so it is routed once.
    .route("/credentials/{id}/limits", get(limit_status::<C>));

    // `/models/{discover,test}` are v3's paths and stay static segments in
    // front of `/models/{id}`: axum prefers the literal, so a model may not be
    // addressed by either name — which is exactly what v3 did too.
    let router = config_family!(router, "/models", [models()], ModelWrite, ModelPatch)
        .route("/models/discover", post(discover_models::<C>))
        .route("/models/discover/apply", post(apply_discovered::<C>))
        .route("/models/test", post(model_test::<C>));

    let router = config_family!(
        router,
        "/provider-models",
        [provider_models()],
        ProviderModelWrite,
        ProviderModelPatch
    );

    let router = config_family!(router, "/routes", [routes()], RouteWrite, RoutePatch);
    let router = config_family!(
        router,
        "/route-members",
        [route_members()],
        RouteMemberWrite,
        RouteMemberPatch
    );
    let router = config_family!(
        router,
        "/exposed-models",
        [exposed_models()],
        ExposedModelWrite,
        ExposedModelPatch
    );
    let router = config_family!(
        router,
        "/connection-profiles",
        [connection_profiles()],
        ConnectionProfileWrite,
        ConnectionProfilePatch
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
        RuleSetPatch
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
        RewriteRulePatch
    );
    let router = config_family!(
        router,
        "/provider-rule-sets",
        [rewrite().bindings()],
        ProviderRuleSetWrite,
        ProviderRuleSetPatch
    );

    let router = config_family!(
        router,
        "/operation-rules",
        [endpoints().operation_rules()],
        OperationRuleWrite,
        OperationRulePatch
    );
    let router = config_family!(
        router,
        "/operation-endpoints",
        [endpoints().operation_endpoints()],
        OperationEndpointWrite,
        OperationEndpointPatch
    );

    let router = config_family!(router, "/quotas", [quotas()], QuotaWrite, QuotaPatch)
        .route("/quotas/status", get(budget_status::<C>))
        .route("/quotas/{id}/reset", post(reset_budget::<C>))
        .route("/quotas/{id}/limit-reset", post(reset_limit::<C>));

    let router = config_family!(
        router,
        "/price-rules",
        [pricing().rules()],
        PriceRuleWrite,
        PriceRulePatch
    );
    let router = config_family!(
        router,
        "/price-rates",
        [pricing().rates()],
        PriceRateWrite,
        PriceRatePatch
    );
    let router = config_family!(
        router,
        "/price-tiers",
        [pricing().tiers()],
        PriceTierWrite,
        PriceTierPatch
    );

    router
        .route("/export", post(export::<C>))
        .route("/import", post(import::<C>))
        .route("/connectivity/test", post(connectivity_test::<C>))
        .route("/channels", get(channels::<C>))
        .route("/tls-presets", get(tls_presets::<C>))
        .route("/rule-presets", get(rule_presets::<C>))
        .route("/default-model-catalog", get(default_models::<C>))
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
    Path(id): Path<String>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
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
}

// ----------------------------------------------------------- credentials --

/// The one read on this surface that is audited.
///
/// It is a `POST` so that it is: the audit middleware deliberately skips safe
/// methods, because a trail that records every list is a trail nobody reads.
/// Disclosing a secret is the exception worth keeping — so it is spelled as an
/// unsafe method and lands in `audit_event` as `admin.credentials.reveal`.
async fn reveal_secret<C>(State(state): State<HostState<C>>, Path(id): Path<String>) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    manage!(state, credentials().reveal_secret(&id))
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
    Path(id): Path<String>,
    Json(body): Json<StatusBody>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    manage!(
        state,
        credentials().set_status(&id, body.status, body.reason)
    )
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
    Path(id): Path<String>,
    Query(query): Query<ForceQuery>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    manage!(state, credentials().refresh(&id, query.mode()))
}

async fn quota_read<C>(State(state): State<HostState<C>>, Path(id): Path<String>) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    manage!(state, credentials().quota_read(&id))
}

async fn quota_probe<C>(State(state): State<HostState<C>>, Path(id): Path<String>) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    manage!(state, credentials().quota_probe(&id))
}

async fn quota_reset<C>(State(state): State<HostState<C>>, Path(id): Path<String>) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    manage!(state, credentials().quota_reset(&id))
}

async fn health_reset<C>(State(state): State<HostState<C>>, Path(id): Path<String>) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    manage!(state, credentials().health_reset(&id))
}

async fn limit_status<C>(State(state): State<HostState<C>>, Path(id): Path<String>) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    manage!(state, credentials().limit_status(&id))
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
    Json(body): Json<DiscoverBody>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    manage!(
        state,
        connectivity().discover_models(&body.provider_id, body.credential_id.as_deref())
    )
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
    Json(body): Json<ApplyDiscoveredBody>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    manage!(
        state,
        connectivity().apply_discovered(&body.provider_id, body.upstream_names)
    )
}

async fn model_test<C>(
    State(state): State<HostState<C>>,
    Json(request): Json<ModelTest>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    manage!(state, connectivity().model_test(request))
}

// -------------------------------------------------------------- settings --

async fn read_settings<C>(State(state): State<HostState<C>>) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    manage!(state, settings().get())
}

async fn patch_settings<C>(
    State(state): State<HostState<C>>,
    Json(patch): Json<SettingsPatch>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    manage!(state, settings().update(patch))
}

// --------------------------------------------------------------- rewrite --

/// The whole set at once, which is why it is a `PUT`: an editor saves a list,
/// and a half-applied reorder is a rule set nobody wrote.
async fn replace_rules<C>(
    State(state): State<HostState<C>>,
    Path(id): Path<String>,
    Json(rules): Json<Vec<RewriteRuleWrite>>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    manage!(state, rewrite().replace_rules(&id, rules))
}

/// Both ids come from the path, so a body cannot point the write at a
/// different rule set than the one the caller addressed.
async fn apply_rule_preset<C>(
    State(state): State<HostState<C>>,
    Path((rule_set_id, preset_id)): Path<(String, String)>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    manage!(
        state,
        catalog().apply_rule_preset(ApplyRulePreset {
            rule_set_id,
            preset_id,
        })
    )
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
    Query(query): Query<OwnersQuery>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
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
    manage!(state, quotas().budget_status(&owners))
}

async fn reset_budget<C>(State(state): State<HostState<C>>, Path(id): Path<String>) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    manage!(state, quotas().reset_budget(&id))
}

/// The other half of the same table: a `credential` or `provider` row is an
/// operator limit rather than a budget, and resetting it unblocks credentials
/// rather than reopening a window.
async fn reset_limit<C>(State(state): State<HostState<C>>, Path(id): Path<String>) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    manage!(state, quotas().reset_limit(&id))
}

// -------------------------------------------------------------- transfer --

/// An export may carry sealed credential blobs, so it is exactly as sensitive
/// as the database file it came from. `no-store` keeps it out of a proxy's and
/// a browser's disk cache; the authorization is the surface's own.
async fn export<C>(
    State(state): State<HostState<C>>,
    Json(request): Json<ExportRequest>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
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
}

async fn import<C>(
    State(state): State<HostState<C>>,
    Json(request): Json<ImportRequest>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    manage!(state, transfer().import(request))
}

// ---------------------------------------------------------- connectivity --

async fn connectivity_test<C>(
    State(state): State<HostState<C>>,
    Json(request): Json<ConnectivityTest>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    manage!(state, connectivity().test(request))
}

// --------------------------------------------------------------- catalog --

/// Every compiled-in channel as data, which is what a console renders a
/// provider form from. `ChannelDescriptor` goes over the wire as itself.
async fn channels<C>(State(state): State<HostState<C>>) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::error::ok_json(&state.app().gproxy().manage().catalog().channels())
}

async fn tls_presets<C>(State(state): State<HostState<C>>) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::error::ok_json(&state.app().gproxy().manage().catalog().tls_presets())
}

async fn rule_presets<C>(State(state): State<HostState<C>>) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::error::ok_json(&state.app().gproxy().manage().catalog().rule_presets())
}

/// The prices and context windows this release was built with. It is parsed
/// from a compiled-in asset rather than read, so there is nothing to await.
async fn default_models<C>(State(state): State<HostState<C>>) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    reply_sdk(state.app().gproxy().manage().catalog().default_models())
}

async fn apply_default_prices<C>(
    State(state): State<HostState<C>>,
    Json(request): Json<ApplyDefaultPricesRequest>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    manage!(state, catalog().apply_default_prices(request))
}

// ------------------------------------------------------------- tokenizer --

async fn vocabularies<C>(State(state): State<HostState<C>>) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    manage!(state, tokenizer().vocabularies())
}

async fn fetch_vocabulary<C>(
    State(state): State<HostState<C>>,
    Json(request): Json<TokenizerFetch>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    manage!(state, tokenizer().fetch(request))
}

/// How far the download running **in this process** has got. A peer's is
/// invisible here, which is why it is progress rather than state.
async fn fetch_progress<C>(State(state): State<HostState<C>>) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::error::ok_json(&state.app().gproxy().manage().tokenizer().progress())
}

async fn delete_vocabulary<C>(State(state): State<HostState<C>>, Path(id): Path<String>) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    manage!(@empty state, tokenizer().delete(&id))
}

async fn tokenizer_auth<C>(State(state): State<HostState<C>>) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    manage!(state, tokenizer().auth())
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
    Json(body): Json<TokenBody>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    manage!(state, tokenizer().set_auth(body.token))
}

/// The other deliberate disclosure, for the same reason as `reveal_secret`:
/// a `POST`, so the trail records it.
async fn reveal_tokenizer_auth<C>(State(state): State<HostState<C>>) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
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
}

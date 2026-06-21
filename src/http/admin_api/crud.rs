//! `edge_crud!` macro + standard CRUD entities for the edge admin dispatcher.
//!
//! Mirrors the semantics of the native `crud_entity!` macro (in
//! `server/admin/crud/entities.rs`) but targets the cross-target [`Resp`] /
//! [`ApiError`] API instead of axum extractors.
//!
//! Entities covered here: orgs, providers, routes, aliases, rule-sets.

use bytes::Bytes;
use http::Method;
use http::request::Parts;

use crate::admin::guard::guard_admin;
use crate::admin::invalidate;
use crate::api::error::ApiError;
use crate::app::AppState;
use crate::store::persistence::records::{
    Alias, AliasInput, Org, OrgInput, Provider, ProviderInput, Route, RouteInput, RuleSet,
    RuleSetInput,
};

use super::{Resp, internal, json_body, parse_i64, segments};

/// `POST /admin/providers` — create/update a provider; on create, seed the
/// channel's default routing rules. Channel is immutable after creation.
/// Resolved by the edge dispatcher before the generic providers CRUD.
pub(super) async fn create_provider_seeded(
    state: &AppState,
    parts: &Parts,
    body: &Bytes,
) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let input: ProviderInput = json_body(body)?;
    let is_create = input.id.is_none();
    if let Some(id) = input.id {
        let existing = state
            .persistence
            .get_provider(id)
            .await
            .map_err(internal)?
            .ok_or_else(|| ApiError::NotFound("provider not found".into()))?;
        if existing.channel != input.channel {
            return Err(ApiError::BadRequest(
                "channel cannot be changed after creation".into(),
            ));
        }
    }
    let rec = state
        .persistence
        .upsert_provider(input)
        .await
        .map_err(ApiError::from_upsert)?;
    if is_create {
        crate::api::routing::seed_default_routing(
            state.persistence.as_ref(),
            state.channels.as_ref(),
            rec.id,
            false,
        )
        .await?;
    }
    invalidate(state).await;
    Resp::json(200, &rec)
}

/// `POST /admin/providers/{id}/routing-rules/reset` — re-seed the channel's
/// default routing rules and return the resulting set.
pub(super) async fn reset_routing(
    state: &AppState,
    parts: &Parts,
    pid: &str,
) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let provider_id = parse_i64(pid)?;
    crate::api::routing::seed_default_routing(
        state.persistence.as_ref(),
        state.channels.as_ref(),
        provider_id,
        true,
    )
    .await?;
    invalidate(state).await;
    let rows = state
        .persistence
        .list_routing_rules(provider_id)
        .await
        .map_err(internal)?;
    Resp::json(200, &rows)
}

// ── edge_crud! macro ─────────────────────────────────────────────────────────

/// Generate a `dispatch_<name>` function that handles the four standard CRUD
/// routes for one entity.
///
/// Each arm: `guard_admin` first, then persistence, then `Resp`.
/// Returns `Option<Result<Resp, ApiError>>`:
///   `Some(Ok(resp))` — handled, success
///   `Some(Err(e))`   — handled, error
///   `None`           — path did not match; fall through
///
/// Two get-by-id strategies:
///   `get  get($getter_fn)` — call `state.persistence.$getter(id)`
///   `get  find`            — scan `list_fn()` and match `.id == id`
macro_rules! edge_crud {
    // ── variant: get via a backend getter ────────────────────────────────────
    (
        fn   $fn_name:ident,
        seg  $seg:literal,
        rec  $record:ty,
        inp  $input:ty,
        list $list_fn:ident,
        get  get($getter:ident),
        ups  $upsert_fn:ident,
        del  $delete_fn:ident $(,)?
    ) => {
        async fn $fn_name(
            state: &AppState,
            parts: &Parts,
            body: &Bytes,
        ) -> Option<Result<Resp, ApiError>> {
            let segs = segments(parts);
            match (&parts.method, segs.as_slice()) {
                // list
                (&Method::GET, ["admin", $seg]) => Some(
                    async {
                        guard_admin(state, parts).await?;
                        let recs: Vec<$record> =
                            state.persistence.$list_fn().await.map_err(internal)?;
                        Resp::json(200, &recs)
                    }
                    .await,
                ),

                // upsert
                (&Method::POST, ["admin", $seg]) => Some(
                    async {
                        guard_admin(state, parts).await?;
                        let input: $input = json_body(body)?;
                        let rec = state
                            .persistence
                            .$upsert_fn(input)
                            .await
                            .map_err(ApiError::from_upsert)?;
                        invalidate(state).await;
                        Resp::json(200, &rec)
                    }
                    .await,
                ),

                // get by id
                (&Method::GET, ["admin", $seg, id]) => Some(
                    async {
                        guard_admin(state, parts).await?;
                        let id = parse_i64(id)?;
                        let rec = state
                            .persistence
                            .$getter(id)
                            .await
                            .map_err(internal)?
                            .ok_or_else(|| ApiError::NotFound("not found".into()))?;
                        Resp::json(200, &rec)
                    }
                    .await,
                ),

                // delete
                (&Method::DELETE, ["admin", $seg, id]) => Some(
                    async {
                        guard_admin(state, parts).await?;
                        let id = parse_i64(id)?;
                        let deleted = state.persistence.$delete_fn(id).await.map_err(internal)?;
                        if deleted {
                            invalidate(state).await;
                            Ok(Resp::no_content())
                        } else {
                            Err(ApiError::NotFound("not found".into()))
                        }
                    }
                    .await,
                ),

                _ => None,
            }
        }
    };

    // ── variant: get via list-scan (find); used for aliases ──────────────────
    (
        fn   $fn_name:ident,
        seg  $seg:literal,
        rec  $record:ty,
        inp  $input:ty,
        list $list_fn:ident,
        get  find,
        ups  $upsert_fn:ident,
        del  $delete_fn:ident $(,)?
    ) => {
        async fn $fn_name(
            state: &AppState,
            parts: &Parts,
            body: &Bytes,
        ) -> Option<Result<Resp, ApiError>> {
            let segs = segments(parts);
            match (&parts.method, segs.as_slice()) {
                // list
                (&Method::GET, ["admin", $seg]) => Some(
                    async {
                        guard_admin(state, parts).await?;
                        let recs: Vec<$record> =
                            state.persistence.$list_fn().await.map_err(internal)?;
                        Resp::json(200, &recs)
                    }
                    .await,
                ),

                // upsert
                (&Method::POST, ["admin", $seg]) => Some(
                    async {
                        guard_admin(state, parts).await?;
                        let input: $input = json_body(body)?;
                        let rec = state
                            .persistence
                            .$upsert_fn(input)
                            .await
                            .map_err(ApiError::from_upsert)?;
                        invalidate(state).await;
                        Resp::json(200, &rec)
                    }
                    .await,
                ),

                // get by id — scan list
                (&Method::GET, ["admin", $seg, id]) => Some(
                    async {
                        guard_admin(state, parts).await?;
                        let id = parse_i64(id)?;
                        let all: Vec<$record> =
                            state.persistence.$list_fn().await.map_err(internal)?;
                        let rec = all
                            .into_iter()
                            .find(|r| r.id == id)
                            .ok_or_else(|| ApiError::NotFound("not found".into()))?;
                        Resp::json(200, &rec)
                    }
                    .await,
                ),

                // delete
                (&Method::DELETE, ["admin", $seg, id]) => Some(
                    async {
                        guard_admin(state, parts).await?;
                        let id = parse_i64(id)?;
                        let deleted = state.persistence.$delete_fn(id).await.map_err(internal)?;
                        if deleted {
                            invalidate(state).await;
                            Ok(Resp::no_content())
                        } else {
                            Err(ApiError::NotFound("not found".into()))
                        }
                    }
                    .await,
                ),

                _ => None,
            }
        }
    };
}

// ── Entity dispatch functions ─────────────────────────────────────────────────

edge_crud!(
    fn   dispatch_orgs,
    seg  "orgs",
    rec  Org,
    inp  OrgInput,
    list list_orgs,
    get  get(get_org),
    ups  upsert_org,
    del  delete_org,
);

edge_crud!(
    fn   dispatch_providers,
    seg  "providers",
    rec  Provider,
    inp  ProviderInput,
    list list_providers,
    get  get(get_provider),
    ups  upsert_provider,
    del  delete_provider,
);

edge_crud!(
    fn   dispatch_routes,
    seg  "routes",
    rec  Route,
    inp  RouteInput,
    list list_routes,
    get  get(get_route),
    ups  upsert_route,
    del  delete_route,
);

edge_crud!(
    fn   dispatch_aliases,
    seg  "aliases",
    rec  Alias,
    inp  AliasInput,
    list list_aliases,
    get  find,
    ups  upsert_alias,
    del  delete_alias,
);

edge_crud!(
    fn   dispatch_rule_sets,
    seg  "rule-sets",
    rec  RuleSet,
    inp  RuleSetInput,
    list list_rule_sets,
    get  get(get_rule_set),
    ups  upsert_rule_set,
    del  delete_rule_set,
);

// ── Sub-dispatcher ────────────────────────────────────────────────────────────

/// Try each standard CRUD entity in order; return the first `Some`.
pub(super) async fn dispatch(
    state: &AppState,
    parts: &Parts,
    body: &Bytes,
) -> Option<Result<Resp, ApiError>> {
    if let Some(r) = dispatch_orgs(state, parts, body).await {
        return Some(r);
    }
    if let Some(r) = dispatch_providers(state, parts, body).await {
        return Some(r);
    }
    if let Some(r) = dispatch_routes(state, parts, body).await {
        return Some(r);
    }
    if let Some(r) = dispatch_aliases(state, parts, body).await {
        return Some(r);
    }
    dispatch_rule_sets(state, parts, body).await
}

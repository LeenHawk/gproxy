//! `edge_crud_nested!` macro + nested CRUD entities for the edge admin
//! dispatcher.
//!
//! Each nested entity has:
//!   `GET  /admin/{parent_prefix}/{parent_id}/{child_prefix}` — list by parent
//!   `POST /admin/{parent_prefix}/{parent_id}/{child_prefix}` — upsert (fk must match)
//!   `DELETE /admin/{delete_prefix}/{id}`                     — delete by id
//!
//! Entities: teams, provider-models, route-members, rules, routing-rules,
//! provider-rule-sets.

use bytes::Bytes;
use http::Method;
use http::request::Parts;

use crate::admin::guard::guard_admin;
use crate::admin::invalidate;
use crate::api::error::ApiError;
use crate::app::AppState;
use crate::store::persistence::records::{
    ProviderModel, ProviderModelInput, ProviderRuleSet, ProviderRuleSetInput, RouteMember,
    RouteMemberInput, RoutingRule, RoutingRuleInput, Rule, RuleInput, Team, TeamInput,
};

use super::{Resp, internal, json_body, parse_i64, segments};

// ── edge_crud_nested! macro ───────────────────────────────────────────────────

/// Generate a `dispatch_<name>` function handling nested CRUD routes.
///
/// Routes matched:
///   `GET  ["admin", parent_prefix, parent_id, child_prefix]` → list
///   `POST ["admin", parent_prefix, parent_id, child_prefix]` → upsert (validates fk)
///   `GET  ["admin", delete_prefix, id]`                      — NOT emitted (no per-id get)
///   `DELETE ["admin", delete_prefix, id]`                    → delete
///
/// `parent_fk` names the field on `Input` that must equal `parent_id`.
macro_rules! edge_crud_nested {
    (
        fn           $fn_name:ident,
        parent_seg   $parent_seg:literal,
        child_seg    $child_seg:literal,
        delete_seg   $delete_seg:literal,
        rec          $record:ty,
        inp          $input:ty,
        parent_fk    $parent_fk:ident,
        list         $list_fn:ident,
        ups          $upsert_fn:ident,
        del          $delete_fn:ident $(,)?
    ) => {
        async fn $fn_name(
            state: &AppState,
            parts: &Parts,
            body: &Bytes,
        ) -> Option<Result<Resp, ApiError>> {
            let segs = segments(parts);
            match (&parts.method, segs.as_slice()) {
                // list by parent
                (&Method::GET, ["admin", $parent_seg, parent_id_seg, $child_seg]) => Some(
                    async {
                        guard_admin(state, parts).await?;
                        let parent_id = parse_i64(parent_id_seg)?;
                        let recs: Vec<$record> = state
                            .persistence
                            .$list_fn(parent_id)
                            .await
                            .map_err(internal)?;
                        Resp::json(200, &recs)
                    }
                    .await,
                ),

                // upsert (validate fk matches URL parent_id)
                (&Method::POST, ["admin", $parent_seg, parent_id_seg, $child_seg]) => Some(
                    async {
                        guard_admin(state, parts).await?;
                        let parent_id = parse_i64(parent_id_seg)?;
                        let input: $input = json_body(body)?;
                        if input.$parent_fk != parent_id {
                            return Err(ApiError::BadRequest(format!(
                                "body {} {} does not match URL parent {}",
                                stringify!($parent_fk),
                                input.$parent_fk,
                                parent_id,
                            )));
                        }
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

                // delete by id
                (&Method::DELETE, ["admin", $delete_seg, id_seg]) => Some(
                    async {
                        guard_admin(state, parts).await?;
                        let id = parse_i64(id_seg)?;
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

// ── Nested entity dispatch functions ─────────────────────────────────────────

// teams: /admin/orgs/{org_id}/teams  —  DELETE /admin/teams/{id}
edge_crud_nested!(
    fn           dispatch_teams,
    parent_seg   "orgs",
    child_seg    "teams",
    delete_seg   "teams",
    rec          Team,
    inp          TeamInput,
    parent_fk    org_id,
    list         list_teams,
    ups          upsert_team,
    del          delete_team,
);

// provider-models: /admin/providers/{provider_id}/models
//                  DELETE /admin/provider-models/{id}
edge_crud_nested!(
    fn           dispatch_provider_models,
    parent_seg   "providers",
    child_seg    "models",
    delete_seg   "provider-models",
    rec          ProviderModel,
    inp          ProviderModelInput,
    parent_fk    provider_id,
    list         list_provider_models,
    ups          upsert_provider_model,
    del          delete_provider_model,
);

// route-members: /admin/routes/{route_id}/members
//                DELETE /admin/route-members/{id}
edge_crud_nested!(
    fn           dispatch_route_members,
    parent_seg   "routes",
    child_seg    "members",
    delete_seg   "route-members",
    rec          RouteMember,
    inp          RouteMemberInput,
    parent_fk    route_id,
    list         list_route_members,
    ups          upsert_route_member,
    del          delete_route_member,
);

// rules: /admin/rule-sets/{rule_set_id}/rules
//        DELETE /admin/rules/{id}
edge_crud_nested!(
    fn           dispatch_rules,
    parent_seg   "rule-sets",
    child_seg    "rules",
    delete_seg   "rules",
    rec          Rule,
    inp          RuleInput,
    parent_fk    rule_set_id,
    list         list_rules,
    ups          upsert_rule,
    del          delete_rule,
);

// routing-rules: /admin/providers/{provider_id}/routing-rules
//                DELETE /admin/routing-rules/{id}
edge_crud_nested!(
    fn           dispatch_routing_rules,
    parent_seg   "providers",
    child_seg    "routing-rules",
    delete_seg   "routing-rules",
    rec          RoutingRule,
    inp          RoutingRuleInput,
    parent_fk    provider_id,
    list         list_routing_rules,
    ups          upsert_routing_rule,
    del          delete_routing_rule,
);

// provider-rule-sets: /admin/providers/{provider_id}/rule-sets
//                     DELETE /admin/provider-rule-sets/{id}
//
// NOTE: The DELETE segment "provider-rule-sets" is disjoint from the standard
// entity's list/upsert segment "rule-sets" (handled by dispatch_rule_sets in
// crud.rs) and from the parent GET/DELETE segment "providers" (3-segment paths),
// so there is no collision.
edge_crud_nested!(
    fn           dispatch_provider_rule_sets,
    parent_seg   "providers",
    child_seg    "rule-sets",
    delete_seg   "provider-rule-sets",
    rec          ProviderRuleSet,
    inp          ProviderRuleSetInput,
    parent_fk    provider_id,
    list         list_provider_rule_sets,
    ups          upsert_provider_rule_set,
    del          delete_provider_rule_set,
);

// ── Sub-dispatcher ────────────────────────────────────────────────────────────

/// Try each nested CRUD entity in order; return the first `Some`.
///
/// Collision analysis with standard `dispatch_providers` (crud.rs):
///   Standard providers:  `["admin","providers"]`        (list)
///                        `["admin","providers", id]`     (get/delete — 3 segs)
///   Nested under providers: `["admin","providers", pid, child]` (4 segs)
/// The macro matches on 4 segments for list/upsert and a *different* delete
/// prefix for the delete arm, so there is no overlap with the 2- or 3-segment
/// standard routes.
pub(super) async fn dispatch(
    state: &AppState,
    parts: &Parts,
    body: &Bytes,
) -> Option<Result<Resp, ApiError>> {
    if let Some(r) = dispatch_teams(state, parts, body).await {
        return Some(r);
    }
    if let Some(r) = dispatch_provider_models(state, parts, body).await {
        return Some(r);
    }
    if let Some(r) = dispatch_route_members(state, parts, body).await {
        return Some(r);
    }
    if let Some(r) = dispatch_rules(state, parts, body).await {
        return Some(r);
    }
    if let Some(r) = dispatch_routing_rules(state, parts, body).await {
        return Some(r);
    }
    dispatch_provider_rule_sets(state, parts, body).await
}

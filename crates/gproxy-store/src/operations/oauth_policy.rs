//! Evaluate persisted policies in the same SQL statement as the protected action.

use crate::entity::{
    config::setting,
    identity::{organization, organization_member, team, team_member, user},
};
use gproxy_seaorm::json_array_contains_text;
use sea_orm::{
    ColumnTrait, Condition, DbBackend, EntityTrait, QueryFilter, QuerySelect, QueryTrait,
    sea_query::{Expr, ExprTrait},
};

/// No configured row at a level means inherit. Otherwise at least one configured
/// row must admit the client (same-level union). All four levels must admit it.
pub(super) fn allowed(
    backend: DbBackend,
    user_id: Expr,
    client_id: Expr,
) -> crate::Result<Condition> {
    let global = setting::Entity::find_by_id(setting::GLOBAL_SETTINGS_ID)
        .filter(setting::Column::OauthClientAllowlist.is_not_null());

    let memberships = team_member::Entity::find()
        .select_only()
        .column(team_member::Column::TeamId)
        .filter(Expr::col((team_member::Entity, team_member::Column::UserId)).eq(user_id.clone()))
        .into_query();
    let teams = team::Entity::find().filter(team::Column::Id.in_subquery(memberships));
    // A team member is also subject to its parent organization's policy, even
    // when no separate organization_members row was provisioned.
    let parent_ids = teams
        .clone()
        .select_only()
        .column(team::Column::OrganizationId)
        .into_query();
    let direct_ids = organization_member::Entity::find()
        .select_only()
        .column(organization_member::Column::OrganizationId)
        .filter(
            Expr::col((
                organization_member::Entity,
                organization_member::Column::UserId,
            ))
            .eq(user_id.clone()),
        )
        .into_query();
    let organizations = organization::Entity::find()
        .filter(
            Condition::any()
                .add(organization::Column::Id.in_subquery(direct_ids))
                .add(organization::Column::Id.in_subquery(parent_ids)),
        )
        .filter(organization::Column::OauthClientAllowlist.is_not_null());
    let teams = teams.filter(team::Column::OauthClientAllowlist.is_not_null());
    let users = user::Entity::find()
        .filter(Expr::col((user::Entity, user::Column::Id)).eq(user_id))
        .filter(user::Column::OauthClientAllowlist.is_not_null());

    macro_rules! level {
        ($rows:ident, $entity:path, $column:path) => {
            Condition::any()
                .add(Expr::exists($rows.clone().into_query()).not())
                .add(Expr::exists(
                    $rows
                        .filter(json_array_contains_text(
                            backend,
                            Expr::col(($entity, $column)),
                            client_id.clone(),
                        )?)
                        .into_query(),
                ))
        };
    }
    Ok(Condition::all()
        .add(level!(
            global,
            setting::Entity,
            setting::Column::OauthClientAllowlist
        ))
        .add(level!(
            organizations,
            organization::Entity,
            organization::Column::OauthClientAllowlist
        ))
        .add(level!(
            teams,
            team::Entity,
            team::Column::OauthClientAllowlist
        ))
        .add(level!(
            users,
            user::Entity,
            user::Column::OauthClientAllowlist
        )))
}

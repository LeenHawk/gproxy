//! Authz-domain libSQL ops.

pub mod quotas;
pub mod rate_limits;
pub mod route_permissions;

use crate::store::libsql::LibsqlClient;
use crate::store::persistence::records::Scope;

/// Drop all scope-bound authz rows (route permissions, rate limits, quotas) for
/// a scope, used by identity cascade deletes.
pub async fn delete_scope_rows(
    client: &LibsqlClient,
    scope: Scope,
    scope_id: i64,
) -> anyhow::Result<()> {
    route_permissions::delete_by_scope(client, scope, scope_id).await?;
    rate_limits::delete_by_scope(client, scope, scope_id).await?;
    quotas::delete_by_scope(client, scope, scope_id).await?;
    Ok(())
}

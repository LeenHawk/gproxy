//! Admin audit-log ops for the `db` backend (append-only).

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, QueryOrder, QuerySelect};

use crate::store::persistence::records::{AuditLog, AuditLogInput};

use crate::store::persistence::db::entities::logs::audit_log;

fn to_record(m: audit_log::Model) -> AuditLog {
    AuditLog {
        id: m.id,
        at: m.at,
        actor_id: m.actor_id,
        actor_name: m.actor_name,
        action: m.action,
        target: m.target,
        status: m.status,
        source_ip: m.source_ip,
        created_at: m.created_at,
    }
}

pub async fn append(conn: &DatabaseConnection, input: AuditLogInput) -> anyhow::Result<AuditLog> {
    let now = crate::store::persistence::db::ops::now_secs();
    let model = audit_log::ActiveModel {
        id: NotSet,
        at: Set(now),
        actor_id: Set(input.actor_id),
        actor_name: Set(input.actor_name),
        action: Set(input.action),
        target: Set(input.target),
        status: Set(input.status),
        source_ip: Set(input.source_ip),
        created_at: Set(now),
    }
    .insert(conn)
    .await?;
    Ok(to_record(model))
}

pub async fn list(conn: &DatabaseConnection, limit: u64) -> anyhow::Result<Vec<AuditLog>> {
    Ok(audit_log::Entity::find()
        .order_by_desc(audit_log::Column::Id)
        .limit(limit)
        .all(conn)
        .await?
        .into_iter()
        .map(to_record)
        .collect())
}

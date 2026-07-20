//! Admin audit-log ops for the `db` backend (append-only).

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect, Select,
};

use crate::store::persistence::records::{AuditLog, AuditLogInput};
use crate::store::persistence::{AuditLogQuery, PageQuery, PageResult};

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

pub async fn query_page(
    conn: &DatabaseConnection,
    q: &AuditLogQuery,
    page: &PageQuery,
) -> anyhow::Result<PageResult<AuditLog>> {
    let total = filtered(q).count(conn).await?;
    let items = filtered(q)
        .order_by_desc(audit_log::Column::Id)
        .offset(page.offset)
        .limit(page.limit)
        .all(conn)
        .await?
        .into_iter()
        .map(to_record)
        .collect();
    Ok(PageResult { items, total })
}

fn filtered(q: &AuditLogQuery) -> Select<audit_log::Entity> {
    use audit_log::Column as A;

    let mut sel = audit_log::Entity::find();
    if let Some(v) = q.at_from {
        sel = sel.filter(A::At.gte(v));
    }
    if let Some(v) = q.at_to {
        sel = sel.filter(A::At.lte(v));
    }
    if let Some(v) = q.actor_id {
        sel = sel.filter(A::ActorId.eq(v));
    }
    if let Some(ref v) = q.action {
        sel = sel.filter(A::Action.contains(v));
    }
    if let Some(ref v) = q.target {
        sel = sel.filter(A::Target.contains(v));
    }
    if let Some(v) = q.status {
        sel = sel.filter(A::Status.eq(v));
    }
    if let Some(ref v) = q.source_ip {
        sel = sel.filter(A::SourceIp.eq(v));
    }
    sel
}

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::store::persistence::db::entities::identity::codex_task_binding;
use crate::store::persistence::records::{CodexTaskBinding, CodexTaskBindingInput};

fn record(model: codex_task_binding::Model) -> anyhow::Result<CodexTaskBinding> {
    Ok(CodexTaskBinding {
        id: model.id,
        provider_id: model.provider_id,
        task_id: model.task_id,
        credential_id: model.credential_id,
        owner_user_id: model.owner_user_id,
        environment_id: model.environment_id,
        summary_json: serde_json::from_str(&model.summary_json)?,
        created_at: model.created_at,
        updated_at: model.updated_at,
    })
}

pub async fn get(
    conn: &DatabaseConnection,
    provider_id: i64,
    task_id: &str,
) -> anyhow::Result<Option<CodexTaskBinding>> {
    codex_task_binding::Entity::find()
        .filter(codex_task_binding::Column::ProviderId.eq(provider_id))
        .filter(codex_task_binding::Column::TaskId.eq(task_id))
        .one(conn)
        .await?
        .map(record)
        .transpose()
}

pub async fn list(
    conn: &DatabaseConnection,
    provider_id: i64,
    owner_user_id: i64,
) -> anyhow::Result<Vec<CodexTaskBinding>> {
    codex_task_binding::Entity::find()
        .filter(codex_task_binding::Column::ProviderId.eq(provider_id))
        .filter(codex_task_binding::Column::OwnerUserId.eq(owner_user_id))
        .all(conn)
        .await?
        .into_iter()
        .map(record)
        .collect()
}

pub async fn upsert(
    conn: &DatabaseConnection,
    input: CodexTaskBindingInput,
) -> anyhow::Result<CodexTaskBinding> {
    let now = crate::store::persistence::db::ops::now_secs();
    let summary = serde_json::to_string(&input.summary_json)?;
    let model = match codex_task_binding::Entity::find()
        .filter(codex_task_binding::Column::ProviderId.eq(input.provider_id))
        .filter(codex_task_binding::Column::TaskId.eq(&input.task_id))
        .one(conn)
        .await?
    {
        Some(existing) => {
            let mut active: codex_task_binding::ActiveModel = existing.into();
            active.credential_id = Set(input.credential_id);
            active.owner_user_id = Set(input.owner_user_id);
            active.environment_id = Set(input.environment_id);
            active.summary_json = Set(summary);
            active.updated_at = Set(now);
            active.update(conn).await?
        }
        None => {
            codex_task_binding::ActiveModel {
                id: NotSet,
                provider_id: Set(input.provider_id),
                task_id: Set(input.task_id),
                credential_id: Set(input.credential_id),
                owner_user_id: Set(input.owner_user_id),
                environment_id: Set(input.environment_id),
                summary_json: Set(summary),
                created_at: Set(now),
                updated_at: Set(now),
            }
            .insert(conn)
            .await?
        }
    };
    record(model)
}

pub async fn delete_by_user(conn: &DatabaseConnection, owner_user_id: i64) -> anyhow::Result<()> {
    codex_task_binding::Entity::delete_many()
        .filter(codex_task_binding::Column::OwnerUserId.eq(owner_user_id))
        .exec(conn)
        .await?;
    Ok(())
}

//! Credential-model-status ops for the `db` backend.

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::sea_query::OnConflict;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::store::persistence::db::entities::provider::credential_model_status;
use crate::store::persistence::records::{CredentialModelStatus, CredentialModelStatusInput};

fn to_record(m: credential_model_status::Model) -> anyhow::Result<CredentialModelStatus> {
    Ok(CredentialModelStatus {
        id: m.id,
        credential_id: m.credential_id,
        channel: m.channel,
        model_id: m.model_id,
        health_kind: m.health_kind,
        health_json: m
            .health_json
            .map(|s| serde_json::from_str(&s))
            .transpose()?,
        checked_at: m.checked_at,
        last_error: m.last_error,
        created_at: m.created_at,
        updated_at: m.updated_at,
    })
}

pub async fn list(
    conn: &DatabaseConnection,
    credential_id: i64,
) -> anyhow::Result<Vec<CredentialModelStatus>> {
    credential_model_status::Entity::find()
        .filter(credential_model_status::Column::CredentialId.eq(credential_id))
        .all(conn)
        .await?
        .into_iter()
        .map(to_record)
        .collect()
}

pub async fn list_all(conn: &DatabaseConnection) -> anyhow::Result<Vec<CredentialModelStatus>> {
    credential_model_status::Entity::find()
        .all(conn)
        .await?
        .into_iter()
        .map(to_record)
        .collect()
}

pub async fn upsert(
    conn: &DatabaseConnection,
    input: CredentialModelStatusInput,
) -> anyhow::Result<CredentialModelStatus> {
    let now = crate::store::persistence::db::ops::now_secs();
    let health = input
        .health_json
        .map(|v| serde_json::to_string(&v))
        .transpose()?;

    if let Some(id) = input.id
        && let Some(existing) = credential_model_status::Entity::find_by_id(id)
            .one(conn)
            .await?
    {
        let mut am: credential_model_status::ActiveModel = existing.into();
        am.credential_id = Set(input.credential_id);
        am.channel = Set(input.channel);
        am.model_id = Set(input.model_id);
        am.health_kind = Set(input.health_kind);
        am.health_json = Set(health);
        am.checked_at = Set(input.checked_at);
        am.last_error = Set(input.last_error);
        am.updated_at = Set(now);
        return to_record(am.update(conn).await?);
    }

    let credential_id = input.credential_id;
    let channel = input.channel;
    let model_id = input.model_id;
    credential_model_status::Entity::insert(credential_model_status::ActiveModel {
        id: NotSet,
        credential_id: Set(credential_id),
        channel: Set(channel.clone()),
        model_id: Set(model_id.clone()),
        health_kind: Set(input.health_kind),
        health_json: Set(health),
        checked_at: Set(input.checked_at),
        last_error: Set(input.last_error),
        created_at: Set(now),
        updated_at: Set(now),
    })
    .on_conflict(
        OnConflict::columns([
            credential_model_status::Column::CredentialId,
            credential_model_status::Column::Channel,
            credential_model_status::Column::ModelId,
        ])
        .update_columns([
            credential_model_status::Column::HealthKind,
            credential_model_status::Column::HealthJson,
            credential_model_status::Column::CheckedAt,
            credential_model_status::Column::LastError,
            credential_model_status::Column::UpdatedAt,
        ])
        .to_owned(),
    )
    .exec(conn)
    .await?;

    let model = credential_model_status::Entity::find()
        .filter(credential_model_status::Column::CredentialId.eq(credential_id))
        .filter(credential_model_status::Column::Channel.eq(channel))
        .filter(credential_model_status::Column::ModelId.eq(model_id))
        .one(conn)
        .await?
        .ok_or_else(|| anyhow::anyhow!("credential model status vanished after upsert"))?;
    to_record(model)
}

pub async fn delete(conn: &DatabaseConnection, id: i64) -> anyhow::Result<bool> {
    let res = credential_model_status::Entity::delete_by_id(id)
        .exec(conn)
        .await?;
    Ok(res.rows_affected > 0)
}

pub async fn delete_by_credential(
    conn: &DatabaseConnection,
    credential_id: i64,
) -> anyhow::Result<()> {
    credential_model_status::Entity::delete_many()
        .filter(credential_model_status::Column::CredentialId.eq(credential_id))
        .exec(conn)
        .await?;
    Ok(())
}

//! Credential-status ops for the `db` backend.

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::store::persistence::records::{CredentialStatus, CredentialStatusInput};

use crate::store::persistence::db::entities::provider::credential_status;

fn to_record(m: credential_status::Model) -> anyhow::Result<CredentialStatus> {
    Ok(CredentialStatus {
        id: m.id,
        credential_id: m.credential_id,
        channel: m.channel,
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
) -> anyhow::Result<Vec<CredentialStatus>> {
    credential_status::Entity::find()
        .filter(credential_status::Column::CredentialId.eq(credential_id))
        .all(conn)
        .await?
        .into_iter()
        .map(to_record)
        .collect()
}

pub async fn list_all(conn: &DatabaseConnection) -> anyhow::Result<Vec<CredentialStatus>> {
    credential_status::Entity::find()
        .all(conn)
        .await?
        .into_iter()
        .map(to_record)
        .collect()
}

pub async fn upsert(
    conn: &DatabaseConnection,
    input: CredentialStatusInput,
) -> anyhow::Result<CredentialStatus> {
    let now = crate::store::persistence::db::ops::now_secs();
    let health = input
        .health_json
        .map(|v| serde_json::to_string(&v))
        .transpose()?;

    // Locate by explicit id, else by (credential_id, channel) uniqueness.
    let existing = match input.id {
        Some(id) => credential_status::Entity::find_by_id(id).one(conn).await?,
        None => {
            credential_status::Entity::find()
                .filter(credential_status::Column::CredentialId.eq(input.credential_id))
                .filter(credential_status::Column::Channel.eq(input.channel.clone()))
                .one(conn)
                .await?
        }
    };

    let model = match existing {
        Some(existing) => {
            let mut am: credential_status::ActiveModel = existing.into();
            am.credential_id = Set(input.credential_id);
            am.channel = Set(input.channel);
            am.health_kind = Set(input.health_kind);
            am.health_json = Set(health);
            am.checked_at = Set(input.checked_at);
            am.last_error = Set(input.last_error);
            am.updated_at = Set(now);
            am.update(conn).await?
        }
        None => {
            credential_status::ActiveModel {
                id: NotSet,
                credential_id: Set(input.credential_id),
                channel: Set(input.channel),
                health_kind: Set(input.health_kind),
                health_json: Set(health),
                checked_at: Set(input.checked_at),
                last_error: Set(input.last_error),
                created_at: Set(now),
                updated_at: Set(now),
            }
            .insert(conn)
            .await?
        }
    };

    to_record(model)
}

pub async fn delete(conn: &DatabaseConnection, id: i64) -> anyhow::Result<bool> {
    let res = credential_status::Entity::delete_by_id(id)
        .exec(conn)
        .await?;
    Ok(res.rows_affected > 0)
}

pub async fn delete_by_credential(
    conn: &DatabaseConnection,
    credential_id: i64,
) -> anyhow::Result<()> {
    credential_status::Entity::delete_many()
        .filter(credential_status::Column::CredentialId.eq(credential_id))
        .exec(conn)
        .await?;
    Ok(())
}

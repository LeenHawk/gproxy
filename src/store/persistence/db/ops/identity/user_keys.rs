//! User-key ops for the `db` backend. `api_key_digest` is unique.

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::store::persistence::records::{UserKey, UserKeyInput};

use crate::store::persistence::db::entities::identity::user_key;

fn to_record(m: user_key::Model) -> UserKey {
    UserKey {
        id: m.id,
        user_id: m.user_id,
        api_key_ciphertext: m.api_key_ciphertext,
        api_key_digest: m.api_key_digest,
        label: m.label,
        enabled: m.enabled,
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

pub async fn list(conn: &DatabaseConnection, user_id: i64) -> anyhow::Result<Vec<UserKey>> {
    Ok(user_key::Entity::find()
        .filter(user_key::Column::UserId.eq(user_id))
        .all(conn)
        .await?
        .into_iter()
        .map(to_record)
        .collect())
}

pub async fn get(conn: &DatabaseConnection, id: i64) -> anyhow::Result<Option<UserKey>> {
    Ok(user_key::Entity::find_by_id(id)
        .one(conn)
        .await?
        .map(to_record))
}

pub async fn find_by_digest(
    conn: &DatabaseConnection,
    digest: &str,
) -> anyhow::Result<Option<UserKey>> {
    Ok(user_key::Entity::find()
        .filter(user_key::Column::ApiKeyDigest.eq(digest))
        .one(conn)
        .await?
        .map(to_record))
}

pub async fn upsert(conn: &DatabaseConnection, input: UserKeyInput) -> anyhow::Result<UserKey> {
    let now = crate::store::persistence::db::ops::now_secs();
    let digest = input.api_key_digest.clone();
    let conflict = |e| {
        crate::store::persistence::db::ops::conflict_if_unique(e, || {
            format!("user key digest already exists: {digest}")
        })
    };

    let model = match input.id {
        Some(id) => match user_key::Entity::find_by_id(id).one(conn).await? {
            Some(existing) => {
                let mut am: user_key::ActiveModel = existing.into();
                am.user_id = Set(input.user_id);
                am.api_key_ciphertext = Set(input.api_key_ciphertext);
                am.api_key_digest = Set(input.api_key_digest);
                am.label = Set(input.label);
                am.enabled = Set(input.enabled);
                am.updated_at = Set(now);
                am.update(conn).await.map_err(conflict)?
            }
            None => {
                // Seeding an empty store from a pinned bundle: insert WITH the
                // explicit id (matches the file backend's insert-with-id).
                user_key::ActiveModel {
                    id: Set(id),
                    user_id: Set(input.user_id),
                    api_key_ciphertext: Set(input.api_key_ciphertext),
                    api_key_digest: Set(input.api_key_digest),
                    label: Set(input.label),
                    enabled: Set(input.enabled),
                    created_at: Set(now),
                    updated_at: Set(now),
                }
                .insert(conn)
                .await
                .map_err(conflict)?
            }
        },
        None => user_key::ActiveModel {
            id: NotSet,
            user_id: Set(input.user_id),
            api_key_ciphertext: Set(input.api_key_ciphertext),
            api_key_digest: Set(input.api_key_digest),
            label: Set(input.label),
            enabled: Set(input.enabled),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(conn)
        .await
        .map_err(conflict)?,
    };

    Ok(to_record(model))
}

pub async fn delete(conn: &DatabaseConnection, id: i64) -> anyhow::Result<bool> {
    let res = user_key::Entity::delete_by_id(id).exec(conn).await?;
    Ok(res.rows_affected > 0)
}

pub async fn delete_by_user(conn: &DatabaseConnection, user_id: i64) -> anyhow::Result<()> {
    user_key::Entity::delete_many()
        .filter(user_key::Column::UserId.eq(user_id))
        .exec(conn)
        .await?;
    Ok(())
}

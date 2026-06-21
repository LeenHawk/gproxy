//! Provider ops for the `db` backend.

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::store::persistence::records::{Provider, ProviderInput};

use crate::store::persistence::db::entities::provider::provider;

fn to_record(m: provider::Model) -> anyhow::Result<Provider> {
    Ok(Provider {
        id: m.id,
        name: m.name,
        channel: m.channel,
        label: m.label,
        settings_json: serde_json::from_str(&m.settings_json)?,
        credential_strategy: m.credential_strategy,
        proxy_url: m.proxy_url,
        tls_fingerprint: m
            .tls_fingerprint
            .map(|s| serde_json::from_str(&s))
            .transpose()?,
        enabled: m.enabled,
        created_at: m.created_at,
        updated_at: m.updated_at,
    })
}

pub async fn list(conn: &DatabaseConnection) -> anyhow::Result<Vec<Provider>> {
    provider::Entity::find()
        .all(conn)
        .await?
        .into_iter()
        .map(to_record)
        .collect()
}

pub async fn get(conn: &DatabaseConnection, id: i64) -> anyhow::Result<Option<Provider>> {
    provider::Entity::find_by_id(id)
        .one(conn)
        .await?
        .map(to_record)
        .transpose()
}

pub async fn get_by_name(
    conn: &DatabaseConnection,
    name: &str,
) -> anyhow::Result<Option<Provider>> {
    provider::Entity::find()
        .filter(provider::Column::Name.eq(name))
        .one(conn)
        .await?
        .map(to_record)
        .transpose()
}

pub async fn upsert(conn: &DatabaseConnection, input: ProviderInput) -> anyhow::Result<Provider> {
    let now = crate::store::persistence::db::ops::now_secs();
    let settings = serde_json::to_string(&input.settings_json)?;
    let tls = input
        .tls_fingerprint
        .map(|v| serde_json::to_string(&v))
        .transpose()?;
    let name = input.name.clone();
    let conflict = |e| {
        crate::store::persistence::db::ops::conflict_if_unique(e, || {
            format!("provider name already exists: {name}")
        })
    };

    let model = match input.id {
        Some(id) => match provider::Entity::find_by_id(id).one(conn).await? {
            Some(existing) => {
                let mut am: provider::ActiveModel = existing.into();
                am.name = Set(input.name);
                am.channel = Set(input.channel);
                am.label = Set(input.label);
                am.settings_json = Set(settings);
                am.credential_strategy = Set(input.credential_strategy);
                am.proxy_url = Set(input.proxy_url);
                am.tls_fingerprint = Set(tls);
                am.enabled = Set(input.enabled);
                am.updated_at = Set(now);
                am.update(conn).await.map_err(conflict)?
            }
            None => {
                // Seeding an empty store from a pinned bundle: insert WITH the
                // explicit id (matches the file backend's insert-with-id).
                let am = provider::ActiveModel {
                    id: Set(id),
                    name: Set(input.name),
                    channel: Set(input.channel),
                    label: Set(input.label),
                    settings_json: Set(settings),
                    credential_strategy: Set(input.credential_strategy),
                    proxy_url: Set(input.proxy_url),
                    tls_fingerprint: Set(tls),
                    enabled: Set(input.enabled),
                    created_at: Set(now),
                    updated_at: Set(now),
                };
                am.insert(conn).await.map_err(conflict)?
            }
        },
        None => {
            let am = provider::ActiveModel {
                id: NotSet,
                name: Set(input.name),
                channel: Set(input.channel),
                label: Set(input.label),
                settings_json: Set(settings),
                credential_strategy: Set(input.credential_strategy),
                proxy_url: Set(input.proxy_url),
                tls_fingerprint: Set(tls),
                enabled: Set(input.enabled),
                created_at: Set(now),
                updated_at: Set(now),
            };
            am.insert(conn).await.map_err(conflict)?
        }
    };

    to_record(model)
}

pub async fn delete(conn: &DatabaseConnection, id: i64) -> anyhow::Result<bool> {
    // cascade: this provider's credentials (and their statuses) and models.
    for cred in super::credentials::list(conn, id).await? {
        super::credential_statuses::delete_by_credential(conn, cred.id).await?;
    }
    super::credentials::delete_by_provider(conn, id).await?;
    super::provider_models::delete_by_provider(conn, id).await?;

    // §8-B2 rules cascade.
    crate::store::persistence::db::ops::transform::routing_rules::delete_by_provider(conn, id)
        .await?;
    crate::store::persistence::db::ops::transform::provider_rule_sets::delete_by_provider(conn, id)
        .await?;

    let res = provider::Entity::delete_by_id(id).exec(conn).await?;
    Ok(res.rows_affected > 0)
}

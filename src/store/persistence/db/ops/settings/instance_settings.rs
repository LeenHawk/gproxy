//! Instance settings ops for the `db` backend.

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::store::persistence::records::{InstanceSettings, InstanceSettingsInput};

use crate::store::persistence::db::entities::settings::instance_setting;

fn to_record(m: instance_setting::Model) -> InstanceSettings {
    InstanceSettings {
        id: m.id,
        instance_name: m.instance_name,
        proxy: m.proxy,
        spoof_emulation: m.spoof_emulation,
        enable_usage: m.enable_usage,
        enable_upstream_log: m.enable_upstream_log,
        enable_upstream_log_body: m.enable_upstream_log_body,
        enable_downstream_log: m.enable_downstream_log,
        enable_downstream_log_body: m.enable_downstream_log_body,
        disable_log_redaction: m.disable_log_redaction,
        enable_tokenizer_download: m.enable_tokenizer_download,
        update_channel: m.update_channel,
        retention_days: m.retention_days,
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

pub async fn list(conn: &DatabaseConnection) -> anyhow::Result<Vec<InstanceSettings>> {
    Ok(instance_setting::Entity::find()
        .all(conn)
        .await?
        .into_iter()
        .map(to_record)
        .collect())
}

pub async fn get(
    conn: &DatabaseConnection,
    instance_name: &str,
) -> anyhow::Result<Option<InstanceSettings>> {
    Ok(instance_setting::Entity::find()
        .filter(instance_setting::Column::InstanceName.eq(instance_name))
        .one(conn)
        .await?
        .map(to_record))
}

pub async fn upsert(
    conn: &DatabaseConnection,
    input: InstanceSettingsInput,
) -> anyhow::Result<InstanceSettings> {
    let now = crate::store::persistence::db::ops::now_secs();
    let instance_name = input.instance_name.clone();
    let conflict = |e| {
        crate::store::persistence::db::ops::conflict_if_unique(e, || {
            format!("instance name already exists: {instance_name}")
        })
    };

    let model = match input.id {
        Some(id) => match instance_setting::Entity::find_by_id(id).one(conn).await? {
            Some(existing) => {
                let mut am: instance_setting::ActiveModel = existing.into();
                am.instance_name = Set(input.instance_name);
                am.proxy = Set(input.proxy);
                am.spoof_emulation = Set(input.spoof_emulation);
                am.enable_usage = Set(input.enable_usage);
                am.enable_upstream_log = Set(input.enable_upstream_log);
                am.enable_upstream_log_body = Set(input.enable_upstream_log_body);
                am.enable_downstream_log = Set(input.enable_downstream_log);
                am.enable_downstream_log_body = Set(input.enable_downstream_log_body);
                am.disable_log_redaction = Set(input.disable_log_redaction);
                am.enable_tokenizer_download = Set(input.enable_tokenizer_download);
                am.update_channel = Set(input.update_channel);
                am.retention_days = Set(input.retention_days);
                am.updated_at = Set(now);
                am.update(conn).await.map_err(conflict)?
            }
            None => {
                // Seeding an empty store from a pinned bundle: insert WITH the
                // explicit id (matches the file backend's insert-with-id).
                instance_setting::ActiveModel {
                    id: Set(id),
                    instance_name: Set(input.instance_name),
                    proxy: Set(input.proxy),
                    spoof_emulation: Set(input.spoof_emulation),
                    enable_usage: Set(input.enable_usage),
                    enable_upstream_log: Set(input.enable_upstream_log),
                    enable_upstream_log_body: Set(input.enable_upstream_log_body),
                    enable_downstream_log: Set(input.enable_downstream_log),
                    enable_downstream_log_body: Set(input.enable_downstream_log_body),
                    disable_log_redaction: Set(input.disable_log_redaction),
                    enable_tokenizer_download: Set(input.enable_tokenizer_download),
                    update_channel: Set(input.update_channel),
                    retention_days: Set(input.retention_days),
                    created_at: Set(now),
                    updated_at: Set(now),
                }
                .insert(conn)
                .await
                .map_err(conflict)?
            }
        },
        None => instance_setting::ActiveModel {
            id: NotSet,
            instance_name: Set(input.instance_name),
            proxy: Set(input.proxy),
            spoof_emulation: Set(input.spoof_emulation),
            enable_usage: Set(input.enable_usage),
            enable_upstream_log: Set(input.enable_upstream_log),
            enable_upstream_log_body: Set(input.enable_upstream_log_body),
            enable_downstream_log: Set(input.enable_downstream_log),
            enable_downstream_log_body: Set(input.enable_downstream_log_body),
            disable_log_redaction: Set(input.disable_log_redaction),
            enable_tokenizer_download: Set(input.enable_tokenizer_download),
            update_channel: Set(input.update_channel),
            retention_days: Set(input.retention_days),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(conn)
        .await
        .map_err(conflict)?,
    };

    Ok(to_record(model))
}

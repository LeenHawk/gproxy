use sea_orm::{
    ActiveModelTrait,
    ActiveValue::{NotSet, Set},
    DbErr,
};
use serde_json::Value as JsonValue;
use time::OffsetDateTime;

use super::SeaOrmStorage;
use super::entities::{credentials, providers, user_keys, users};

fn parse_json(field: &str, raw: &str) -> Result<JsonValue, DbErr> {
    serde_json::from_str(raw)
        .map_err(|err| DbErr::Custom(format!("invalid json for {field}: {err}")))
}

fn parse_optional_json(field: &str, raw: Option<&str>) -> Result<Option<JsonValue>, DbErr> {
    raw.map(|value| parse_json(field, value)).transpose()
}

impl SeaOrmStorage {
    pub async fn create_provider(
        &self,
        name: &str,
        channel: &str,
        settings_json: &str,
        dispatch_json: &str,
        enabled: bool,
    ) -> Result<i64, DbErr> {
        let now = OffsetDateTime::now_utc();
        let model = providers::ActiveModel {
            id: NotSet,
            name: Set(name.to_string()),
            channel: Set(channel.to_string()),
            settings_json: Set(parse_json("provider.settings_json", settings_json)?),
            dispatch_json: Set(parse_json("provider.dispatch_json", dispatch_json)?),
            enabled: Set(enabled),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(self.connection())
        .await?;
        Ok(model.id)
    }

    pub async fn create_credential(
        &self,
        provider_id: i64,
        name: Option<&str>,
        kind: &str,
        settings_json: Option<&str>,
        secret_json: &str,
        enabled: bool,
    ) -> Result<i64, DbErr> {
        let now = OffsetDateTime::now_utc();
        let model = credentials::ActiveModel {
            id: NotSet,
            provider_id: Set(provider_id),
            name: Set(name.map(str::to_string)),
            kind: Set(kind.to_string()),
            settings_json: Set(parse_optional_json(
                "credential.settings_json",
                settings_json,
            )?),
            secret_json: Set(parse_json("credential.secret_json", secret_json)?),
            enabled: Set(enabled),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(self.connection())
        .await?;
        Ok(model.id)
    }

    pub async fn create_user(
        &self,
        name: &str,
        password: &str,
        enabled: bool,
    ) -> Result<i64, DbErr> {
        let now = OffsetDateTime::now_utc();
        let model = users::ActiveModel {
            id: NotSet,
            name: Set(name.to_string()),
            password: Set(Some(password.to_string())),
            enabled: Set(enabled),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(self.connection())
        .await?;
        Ok(model.id)
    }

    pub async fn create_user_key(
        &self,
        user_id: i64,
        api_key: &str,
        label: Option<&str>,
        enabled: bool,
    ) -> Result<i64, DbErr> {
        let now = OffsetDateTime::now_utc();
        let model = user_keys::ActiveModel {
            id: NotSet,
            user_id: Set(user_id),
            api_key: Set(api_key.to_string()),
            label: Set(label.map(str::to_string)),
            enabled: Set(enabled),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(self.connection())
        .await?;
        Ok(model.id)
    }
}

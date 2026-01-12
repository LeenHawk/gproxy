use anyhow::Result;
use async_trait::async_trait;
use sea_orm::entity::prelude::*;
use sea_orm::{ActiveModelTrait, ConnectionTrait, EntityTrait, Set};
use url::Url;

use crate::providers::credential_index;
use crate::providers::antigravity::{AntigravityBackend, AntigravityCredential, AntigravitySetting};
use crate::providers::credential_status::{deserialize_states_json, serialize_states_json};
use crate::storage::{ConfigStore, DatabaseStorage};

pub mod config {
    use sea_orm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "antigravity_config")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i32,
        pub base_url: String,
        pub rotate_num: i32,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod credential {
    use sea_orm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "antigravity_credentials")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub project_id: String,
        pub client_email: String,
        pub client_id: String,
        pub client_secret: String,
        pub token: String,
        pub refresh_token: String,
        pub scope: String,
        pub token_uri: String,
        pub expiry: String,
        pub states: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub async fn load_setting<C>(conn: &C) -> Result<AntigravitySetting>
where
    C: ConnectionTrait,
{
    let model = config::Entity::find_by_id(1).one(conn).await?;
    if let Some(model) = model {
        return Ok(AntigravitySetting {
            base_url: Url::parse(&model.base_url)?,
            rotate_num: model.rotate_num as u32,
        });
    }

    let setting = AntigravitySetting::default();
    write_setting(conn, &setting).await?;
    Ok(setting)
}

pub async fn load_credentials<C>(conn: &C) -> Result<Vec<AntigravityCredential>>
where
    C: ConnectionTrait,
{
    let rows = credential::Entity::find().all(conn).await?;
    let credentials = rows
        .into_iter()
        .map(|model| AntigravityCredential {
            project_id: model.project_id,
            client_email: model.client_email,
            client_id: model.client_id,
            client_secret: model.client_secret,
            token: model.token,
            refresh_token: model.refresh_token,
            scope: deserialize_scope(&model.scope),
            token_uri: model.token_uri,
            expiry: model.expiry,
            states: deserialize_states_json(&model.states),
        })
        .collect();
    Ok(credentials)
}

pub async fn write_setting<C>(conn: &C, setting: &AntigravitySetting) -> Result<()>
where
    C: ConnectionTrait,
{
    let model = config::Entity::find_by_id(1).one(conn).await?;
    if let Some(model) = model {
        let mut active: config::ActiveModel = model.into();
        active.base_url = Set(setting.base_url.to_string());
        active.rotate_num = Set(setting.rotate_num as i32);
        active.update(conn).await?;
    } else {
        let active = config::ActiveModel {
            id: Set(1),
            base_url: Set(setting.base_url.to_string()),
            rotate_num: Set(setting.rotate_num as i32),
        };
        active.insert(conn).await?;
    }
    Ok(())
}

pub async fn write_credentials<C>(conn: &C, credentials: &[AntigravityCredential]) -> Result<()>
where
    C: ConnectionTrait,
{
    credential::Entity::delete_many().exec(conn).await?;

    for credential in credentials {
        let active = credential::ActiveModel {
            project_id: Set(credential.project_id.clone()),
            client_email: Set(credential.client_email.clone()),
            client_id: Set(credential.client_id.clone()),
            client_secret: Set(credential.client_secret.clone()),
            token: Set(credential.token.clone()),
            refresh_token: Set(credential.refresh_token.clone()),
            scope: Set(serialize_scope(&credential.scope)),
            token_uri: Set(credential.token_uri.clone()),
            expiry: Set(credential.expiry.clone()),
            states: Set(serialize_states_json(&credential.states)),
        };
        active.insert(conn).await?;
    }

    Ok(())
}

fn serialize_scope(scope: &[String]) -> String {
    scope.join(" ")
}

fn deserialize_scope(scope: &str) -> Vec<String> {
    scope
        .split_whitespace()
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .collect()
}

#[async_trait]
impl AntigravityBackend for DatabaseStorage {
    async fn get_config(&self) -> Result<AntigravitySetting> {
        let config = self.get_app_config().await?;
        Ok(config.providers.antigravity.setting)
    }

    async fn load_config(&self) -> Result<AntigravitySetting> {
        let config = self.load_app_config().await?;
        Ok(config.providers.antigravity.setting)
    }

    async fn update_config<F>(&self, update: F) -> Result<()>
    where
        F: FnOnce(&mut AntigravitySetting) + Send,
    {
        let _guard = self.lock_update().await;
        let config = self.update_cached_config(|config| {
            update(&mut config.providers.antigravity.setting);
        });
        self.save_app_config(&config).await
    }

    async fn get_credentials(&self) -> Result<Vec<AntigravityCredential>> {
        let config = self.get_app_config().await?;
        Ok(config.providers.antigravity.credentials)
    }

    async fn load_credentials(&self) -> Result<Vec<AntigravityCredential>> {
        let config = self.load_app_config().await?;
        Ok(config.providers.antigravity.credentials)
    }

    async fn add_credential(&self, credential: AntigravityCredential) -> Result<()> {
        let _guard = self.lock_update().await;
        let config = self.update_cached_config(|config| {
            let provider = &mut config.providers.antigravity;
            credential_index::ensure_index(&mut provider.credential_index, &provider.credentials);
            let key = credential.project_id.trim().to_string();
            provider.credentials.push(credential);
            if !key.is_empty() {
                provider
                    .credential_index
                    .insert(key, provider.credentials.len() - 1);
            }
        });
        self.save_app_config(&config).await
    }

    async fn update_credential<F>(&self, index: usize, update: F) -> Result<()>
    where
        F: FnOnce(&mut AntigravityCredential) + Send,
    {
        let _guard = self.lock_update().await;
        let config = self.update_cached_config(|config| {
            let provider = &mut config.providers.antigravity;
            credential_index::ensure_index(&mut provider.credential_index, &provider.credentials);
            let Some(credential) = provider.credentials.get_mut(index) else {
                return;
            };
            let old_key = credential.project_id.clone();
            update(credential);
            let new_key = credential.project_id.clone();
            credential_index::update_index_on_change(
                &mut provider.credential_index,
                &old_key,
                &new_key,
                index,
            );
        });
        self.save_app_config(&config).await
    }

    async fn update_credential_by_id<F>(&self, id: &str, update: F) -> Result<()>
    where
        F: FnOnce(&mut AntigravityCredential) + Send,
    {
        let _guard = self.lock_update().await;
        let config = self.update_cached_config(|config| {
            let provider = &mut config.providers.antigravity;
            let Some(index) = credential_index::find_or_rebuild(
                &mut provider.credential_index,
                &provider.credentials,
                id,
            ) else {
                return;
            };
            let Some(credential) = provider.credentials.get_mut(index) else {
                return;
            };
            let old_key = credential.project_id.clone();
            update(credential);
            let new_key = credential.project_id.clone();
            credential_index::update_index_on_change(
                &mut provider.credential_index,
                &old_key,
                &new_key,
                index,
            );
        });
        self.save_app_config(&config).await
    }

    async fn delete_credential(&self, key: &str) -> Result<()> {
        let _guard = self.lock_update().await;
        let config = self.update_cached_config(|config| {
            let provider = &mut config.providers.antigravity;
            provider
                .credentials
                .retain(|cred| cred.project_id != key);
            provider.rebuild_credential_index();
        });
        self.save_app_config(&config).await
    }

    async fn get_credential(&self, index: usize) -> Result<Option<AntigravityCredential>> {
        let config = self.get_app_config().await?;
        Ok(config.providers.antigravity.credentials.get(index).cloned())
    }
}

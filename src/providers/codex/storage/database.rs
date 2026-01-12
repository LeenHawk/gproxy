use anyhow::Result;
use async_trait::async_trait;
use sea_orm::entity::prelude::*;
use sea_orm::{ActiveModelTrait, ConnectionTrait, EntityTrait, Set};
use url::Url;

use crate::providers::credential_index;
use crate::providers::codex::{CodexBackend, CodexCredential, CodexSetting};
use crate::providers::credential_status::{deserialize_states_json, serialize_states_json};
use crate::storage::{ConfigStore, DatabaseStorage};

pub mod config {
    use sea_orm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "codex_config")]
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
    #[sea_orm(table_name = "codex_credentials")]
    pub struct Model {
        pub id_token: String,
        pub access_token: String,
        #[sea_orm(primary_key)]
        pub refresh_token: String,
        pub account_id: String,
        pub last_refresh: i64,
        pub states: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub async fn load_setting<C>(conn: &C) -> Result<CodexSetting>
where
    C: ConnectionTrait,
{
    let model = config::Entity::find_by_id(1).one(conn).await?;
    if let Some(model) = model {
        return Ok(CodexSetting {
            base_url: Url::parse(&model.base_url)?,
            rotate_num: model.rotate_num as u32,
        });
    }

    let setting = CodexSetting::default();
    write_setting(conn, &setting).await?;
    Ok(setting)
}

pub async fn load_credentials<C>(conn: &C) -> Result<Vec<CodexCredential>>
where
    C: ConnectionTrait,
{
    let rows = credential::Entity::find().all(conn).await?;
    let credentials = rows
        .into_iter()
        .map(|model| CodexCredential {
            id_token: model.id_token,
            access_token: model.access_token,
            refresh_token: model.refresh_token,
            account_id: model.account_id,
            last_refresh: model.last_refresh,
            states: deserialize_states_json(&model.states),
        })
        .collect();
    Ok(credentials)
}

pub async fn write_setting<C>(conn: &C, setting: &CodexSetting) -> Result<()>
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

pub async fn write_credentials<C>(conn: &C, credentials: &[CodexCredential]) -> Result<()>
where
    C: ConnectionTrait,
{
    credential::Entity::delete_many().exec(conn).await?;

    for credential in credentials {
        let active = credential::ActiveModel {
            id_token: Set(credential.id_token.clone()),
            access_token: Set(credential.access_token.clone()),
            refresh_token: Set(credential.refresh_token.clone()),
            account_id: Set(credential.account_id.clone()),
            last_refresh: Set(credential.last_refresh),
            states: Set(serialize_states_json(&credential.states)),
        };
        active.insert(conn).await?;
    }

    Ok(())
}

#[async_trait]
impl CodexBackend for DatabaseStorage {
    async fn get_config(&self) -> Result<CodexSetting> {
        let config = self.get_app_config().await?;
        Ok(config.providers.codex.setting)
    }

    async fn load_config(&self) -> Result<CodexSetting> {
        let config = self.load_app_config().await?;
        Ok(config.providers.codex.setting)
    }

    async fn update_config<F>(&self, update: F) -> Result<()>
    where
        F: FnOnce(&mut CodexSetting) + Send,
    {
        let _guard = self.lock_update().await;
        let config = self.update_cached_config(|config| {
            update(&mut config.providers.codex.setting);
        });
        self.save_app_config(&config).await
    }

    async fn get_credentials(&self) -> Result<Vec<CodexCredential>> {
        let config = self.get_app_config().await?;
        Ok(config.providers.codex.credentials)
    }

    async fn load_credentials(&self) -> Result<Vec<CodexCredential>> {
        let config = self.load_app_config().await?;
        Ok(config.providers.codex.credentials)
    }

    async fn add_credential(&self, credential: CodexCredential) -> Result<()> {
        let _guard = self.lock_update().await;
        let config = self.update_cached_config(|config| {
            let provider = &mut config.providers.codex;
            credential_index::ensure_index(&mut provider.credential_index, &provider.credentials);
            let key = credential.refresh_token.trim().to_string();
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
        F: FnOnce(&mut CodexCredential) + Send,
    {
        let _guard = self.lock_update().await;
        let config = self.update_cached_config(|config| {
            let provider = &mut config.providers.codex;
            credential_index::ensure_index(&mut provider.credential_index, &provider.credentials);
            let Some(credential) = provider.credentials.get_mut(index) else {
                return;
            };
            let old_key = credential.refresh_token.clone();
            update(credential);
            let new_key = credential.refresh_token.clone();
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
        F: FnOnce(&mut CodexCredential) + Send,
    {
        let _guard = self.lock_update().await;
        let config = self.update_cached_config(|config| {
            let provider = &mut config.providers.codex;
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
            let old_key = credential.refresh_token.clone();
            update(credential);
            let new_key = credential.refresh_token.clone();
            credential_index::update_index_on_change(
                &mut provider.credential_index,
                &old_key,
                &new_key,
                index,
            );
        });
        self.save_app_config(&config).await
    }

    async fn delete_credential(&self, refresh_token: &str) -> Result<()> {
        let _guard = self.lock_update().await;
        let config = self.update_cached_config(|config| {
            let provider = &mut config.providers.codex;
            provider
                .credentials
                .retain(|cred| cred.refresh_token != refresh_token);
            provider.rebuild_credential_index();
        });
        self.save_app_config(&config).await
    }

    async fn get_credential(&self, index: usize) -> Result<Option<CodexCredential>> {
        let config = self.get_app_config().await?;
        Ok(config.providers.codex.credentials.get(index).cloned())
    }
}

use anyhow::Result;
use async_trait::async_trait;
use sea_orm::entity::prelude::*;
use sea_orm::{ActiveModelTrait, ConnectionTrait, EntityTrait, Set};
use url::Url;

use crate::providers::credential_index;
use crate::providers::claude::{ClaudeBackend, ClaudeCredential, ClaudeSetting};
use crate::providers::credential_status::{deserialize_states_json, serialize_states_json};
use crate::storage::{ConfigStore, DatabaseStorage};

pub mod config {
    use sea_orm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "claude_config")]
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
    #[sea_orm(table_name = "claude_credentials")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub key: String,
        pub states: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub async fn load_setting<C>(conn: &C) -> Result<ClaudeSetting>
where
    C: ConnectionTrait,
{
    let model = config::Entity::find_by_id(1).one(conn).await?;
    if let Some(model) = model {
        return Ok(ClaudeSetting {
            base_url: Url::parse(&model.base_url)?,
            rotate_num: model.rotate_num as u32,
        });
    }

    let setting = ClaudeSetting::default();
    write_setting(conn, &setting).await?;
    Ok(setting)
}

pub async fn load_credentials<C>(conn: &C) -> Result<Vec<ClaudeCredential>>
where
    C: ConnectionTrait,
{
    let rows = credential::Entity::find().all(conn).await?;
    let credentials = rows
        .into_iter()
        .map(|model| ClaudeCredential {
            key: model.key,
            states: deserialize_states_json(&model.states),
        })
        .collect();
    Ok(credentials)
}

pub async fn write_setting<C>(conn: &C, setting: &ClaudeSetting) -> Result<()>
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

pub async fn write_credentials<C>(conn: &C, credentials: &[ClaudeCredential]) -> Result<()>
where
    C: ConnectionTrait,
{
    credential::Entity::delete_many().exec(conn).await?;

    for credential in credentials {
        let active = credential::ActiveModel {
            key: Set(credential.key.clone()),
            states: Set(serialize_states_json(&credential.states)),
        };
        active.insert(conn).await?;
    }

    Ok(())
}

#[async_trait]
impl ClaudeBackend for DatabaseStorage {
    async fn get_config(&self) -> Result<ClaudeSetting> {
        let config = self.get_app_config().await?;
        Ok(config.providers.claude.setting)
    }

    async fn load_config(&self) -> Result<ClaudeSetting> {
        let config = self.load_app_config().await?;
        Ok(config.providers.claude.setting)
    }

    async fn update_config<F>(&self, update: F) -> Result<()>
    where
        F: FnOnce(&mut ClaudeSetting) + Send,
    {
        let _guard = self.lock_update().await;
        let config = self.update_cached_config(|config| {
            update(&mut config.providers.claude.setting);
        });
        self.save_app_config(&config).await
    }

    async fn get_credentials(&self) -> Result<Vec<ClaudeCredential>> {
        let config = self.get_app_config().await?;
        Ok(config.providers.claude.credentials)
    }

    async fn load_credentials(&self) -> Result<Vec<ClaudeCredential>> {
        let config = self.load_app_config().await?;
        Ok(config.providers.claude.credentials)
    }

    async fn add_credential(&self, credential: ClaudeCredential) -> Result<()> {
        let _guard = self.lock_update().await;
        let config = self.update_cached_config(|config| {
            let provider = &mut config.providers.claude;
            credential_index::ensure_index(&mut provider.credential_index, &provider.credentials);
            let key = credential.key.trim().to_string();
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
        F: FnOnce(&mut ClaudeCredential) + Send,
    {
        let _guard = self.lock_update().await;
        let config = self.update_cached_config(|config| {
            let provider = &mut config.providers.claude;
            credential_index::ensure_index(&mut provider.credential_index, &provider.credentials);
            let Some(credential) = provider.credentials.get_mut(index) else {
                return;
            };
            let old_key = credential.key.clone();
            update(credential);
            let new_key = credential.key.clone();
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
        F: FnOnce(&mut ClaudeCredential) + Send,
    {
        let _guard = self.lock_update().await;
        let config = self.update_cached_config(|config| {
            let provider = &mut config.providers.claude;
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
            let old_key = credential.key.clone();
            update(credential);
            let new_key = credential.key.clone();
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
            let provider = &mut config.providers.claude;
            provider.credentials.retain(|cred| cred.key != key);
            provider.rebuild_credential_index();
        });
        self.save_app_config(&config).await
    }

    async fn get_credential(&self, index: usize) -> Result<Option<ClaudeCredential>> {
        let config = self.get_app_config().await?;
        Ok(config.providers.claude.credentials.get(index).cloned())
    }
}

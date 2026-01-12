use anyhow::Result;
use async_trait::async_trait;
use sea_orm::entity::prelude::*;
use sea_orm::{ActiveModelTrait, ConnectionTrait, EntityTrait, Set};
use url::Url;

use crate::providers::credential_index;
use crate::providers::vertex::{VertexBackend, VertexCredential, VertexSetting};
use crate::providers::credential_status::{deserialize_states_json, serialize_states_json};
use crate::storage::{ConfigStore, DatabaseStorage};

pub mod config {
    use sea_orm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "vertex_config")]
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
    #[sea_orm(table_name = "vertex_credentials")]
    pub struct Model {
        #[sea_orm(column_name = "type")]
        pub r#type: String,
        #[sea_orm(primary_key)]
        pub project_id: String,
        pub private_key: String,
        pub client_email: String,
        pub client_id: String,
        pub auth_uri: String,
        pub token_uri: String,
        pub auth_provider_x509_cert_url: String,
        pub client_x509_cert_url: String,
        pub universe_domain: String,
        pub access_token: String,
        pub expires_at: i64,
        pub states: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub async fn load_setting<C>(conn: &C) -> Result<VertexSetting>
where
    C: ConnectionTrait,
{
    let model = config::Entity::find_by_id(1).one(conn).await?;
    if let Some(model) = model {
        return Ok(VertexSetting {
            base_url: Url::parse(&model.base_url)?,
            rotate_num: model.rotate_num as u32,
        });
    }

    let setting = VertexSetting::default();
    write_setting(conn, &setting).await?;
    Ok(setting)
}

pub async fn load_credentials<C>(conn: &C) -> Result<Vec<VertexCredential>>
where
    C: ConnectionTrait,
{
    let rows = credential::Entity::find().all(conn).await?;
    let credentials = rows
        .into_iter()
        .map(|model| VertexCredential {
            r#type: model.r#type,
            project_id: model.project_id,
            private_key: model.private_key,
            client_email: model.client_email,
            client_id: model.client_id,
            auth_uri: model.auth_uri,
            token_uri: model.token_uri,
            auth_provider_x509_cert_url: model.auth_provider_x509_cert_url,
            client_x509_cert_url: model.client_x509_cert_url,
            universe_domain: model.universe_domain,
            access_token: model.access_token,
            expires_at: model.expires_at,
            states: deserialize_states_json(&model.states),
        })
        .collect();
    Ok(credentials)
}

pub async fn write_setting<C>(conn: &C, setting: &VertexSetting) -> Result<()>
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

pub async fn write_credentials<C>(conn: &C, credentials: &[VertexCredential]) -> Result<()>
where
    C: ConnectionTrait,
{
    credential::Entity::delete_many().exec(conn).await?;

    for credential in credentials {
        let active = credential::ActiveModel {
            r#type: Set(credential.r#type.clone()),
            project_id: Set(credential.project_id.clone()),
            private_key: Set(credential.private_key.clone()),
            client_email: Set(credential.client_email.clone()),
            client_id: Set(credential.client_id.clone()),
            auth_uri: Set(credential.auth_uri.clone()),
            token_uri: Set(credential.token_uri.clone()),
            auth_provider_x509_cert_url: Set(credential.auth_provider_x509_cert_url.clone()),
            client_x509_cert_url: Set(credential.client_x509_cert_url.clone()),
            universe_domain: Set(credential.universe_domain.clone()),
            access_token: Set(credential.access_token.clone()),
            expires_at: Set(credential.expires_at),
            states: Set(serialize_states_json(&credential.states)),
        };
        active.insert(conn).await?;
    }

    Ok(())
}

#[async_trait]
impl VertexBackend for DatabaseStorage {
    async fn get_config(&self) -> Result<VertexSetting> {
        let config = self.get_app_config().await?;
        Ok(config.providers.vertex.setting)
    }

    async fn load_config(&self) -> Result<VertexSetting> {
        let config = self.load_app_config().await?;
        Ok(config.providers.vertex.setting)
    }

    async fn update_config<F>(&self, update: F) -> Result<()>
    where
        F: FnOnce(&mut VertexSetting) + Send,
    {
        let _guard = self.lock_update().await;
        let config = self.update_cached_config(|config| {
            update(&mut config.providers.vertex.setting);
        });
        self.save_app_config(&config).await
    }

    async fn get_credentials(&self) -> Result<Vec<VertexCredential>> {
        let config = self.get_app_config().await?;
        Ok(config.providers.vertex.credentials)
    }

    async fn load_credentials(&self) -> Result<Vec<VertexCredential>> {
        let config = self.load_app_config().await?;
        Ok(config.providers.vertex.credentials)
    }

    async fn add_credential(&self, credential: VertexCredential) -> Result<()> {
        let _guard = self.lock_update().await;
        let config = self.update_cached_config(|config| {
            let provider = &mut config.providers.vertex;
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
        F: FnOnce(&mut VertexCredential) + Send,
    {
        let _guard = self.lock_update().await;
        let config = self.update_cached_config(|config| {
            let provider = &mut config.providers.vertex;
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
        F: FnOnce(&mut VertexCredential) + Send,
    {
        let _guard = self.lock_update().await;
        let config = self.update_cached_config(|config| {
            let provider = &mut config.providers.vertex;
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
            let provider = &mut config.providers.vertex;
            provider
                .credentials
                .retain(|cred| cred.project_id != key);
            provider.rebuild_credential_index();
        });
        self.save_app_config(&config).await
    }

    async fn get_credential(&self, index: usize) -> Result<Option<VertexCredential>> {
        let config = self.get_app_config().await?;
        Ok(config.providers.vertex.credentials.get(index).cloned())
    }
}

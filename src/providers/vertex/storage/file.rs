use anyhow::Result;
use async_trait::async_trait;
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, Value};

use crate::providers::credential_index;
use crate::providers::vertex::{VertexBackend, VertexCredential, VertexSetting};
use crate::storage::{ConfigStore, FileStorage};
use crate::storage::toml_states::states_to_item;

#[async_trait]
impl VertexBackend for FileStorage {
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
        let mut updated = None;
        let config = self.update_cached_config(|config| {
            update(&mut config.providers.vertex.setting);
            updated = Some(config.providers.vertex.setting.clone());
        });
        let setting = updated.unwrap_or(config.providers.vertex.setting);
        self.with_document_mut(|doc| {
            write_setting(doc, &setting);
            Ok(())
        })
        .await
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
        self.with_document_mut(|doc| {
            write_credentials(doc, &config.providers.vertex.credentials);
            Ok(())
        })
        .await
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
        self.with_document_mut(|doc| {
            write_credentials(doc, &config.providers.vertex.credentials);
            Ok(())
        })
        .await
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
        self.with_document_mut(|doc| {
            write_credentials(doc, &config.providers.vertex.credentials);
            Ok(())
        })
        .await
    }

    async fn delete_credential(&self, project_id: &str) -> Result<()> {
        let _guard = self.lock_update().await;
        let config = self.update_cached_config(|config| {
            let provider = &mut config.providers.vertex;
            provider
                .credentials
                .retain(|cred| cred.project_id != project_id);
            provider.rebuild_credential_index();
        });
        self.with_document_mut(|doc| {
            write_credentials(doc, &config.providers.vertex.credentials);
            Ok(())
        })
        .await
    }

    async fn get_credential(&self, index: usize) -> Result<Option<VertexCredential>> {
        let config = self.get_app_config().await?;
        Ok(config.providers.vertex.credentials.get(index).cloned())
    }
}

fn write_setting(doc: &mut DocumentMut, setting: &VertexSetting) {
    let setting_item = doc["providers"]["vertex"]["setting"].or_insert(Item::Table(Table::new()));
    let setting_table = setting_item.as_table_mut().expect("setting table");
    setting_table["base_url"] = Value::from(setting.base_url.to_string()).into();
    setting_table["rotate_num"] = Value::from(setting.rotate_num as i64).into();
}

fn write_credentials(doc: &mut DocumentMut, credentials: &[VertexCredential]) {
    let mut array = ArrayOfTables::new();
    for credential in credentials {
        let mut table = Table::new();
        table["type"] = Value::from(credential.r#type.clone()).into();
        table["project_id"] = Value::from(credential.project_id.clone()).into();
        table["private_key"] = Value::from(credential.private_key.clone()).into();
        table["client_email"] = Value::from(credential.client_email.clone()).into();
        table["client_id"] = Value::from(credential.client_id.clone()).into();
        table["auth_uri"] = Value::from(credential.auth_uri.clone()).into();
        table["token_uri"] = Value::from(credential.token_uri.clone()).into();
        table["auth_provider_x509_cert_url"] =
            Value::from(credential.auth_provider_x509_cert_url.clone()).into();
        table["client_x509_cert_url"] = Value::from(credential.client_x509_cert_url.clone()).into();
        table["universe_domain"] = Value::from(credential.universe_domain.clone()).into();
        table["access_token"] = Value::from(credential.access_token.clone()).into();
        table["expires_at"] = Value::from(credential.expires_at).into();
        if let Some(states) = states_to_item(&credential.states) {
            table["states"] = states;
        }
        array.push(table);
    }
    doc["providers"]["vertex"]["credentials"] = Item::ArrayOfTables(array);
}

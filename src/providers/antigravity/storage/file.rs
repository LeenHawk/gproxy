use anyhow::Result;
use async_trait::async_trait;
use toml_edit::{Array, ArrayOfTables, DocumentMut, Item, Table, Value};

use crate::providers::credential_index;
use crate::providers::antigravity::{AntigravityBackend, AntigravityCredential, AntigravitySetting};
use crate::storage::{ConfigStore, FileStorage};
use crate::storage::toml_states::states_to_item;

#[async_trait]
impl AntigravityBackend for FileStorage {
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
        let mut updated = None;
        let config = self.update_cached_config(|config| {
            update(&mut config.providers.antigravity.setting);
            updated = Some(config.providers.antigravity.setting.clone());
        });
        let setting = updated.unwrap_or(config.providers.antigravity.setting);
        self.with_document_mut(|doc| {
            write_setting(doc, &setting);
            Ok(())
        })
        .await
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
        self.with_document_mut(|doc| {
            write_credentials(doc, &config.providers.antigravity.credentials);
            Ok(())
        })
        .await
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
        self.with_document_mut(|doc| {
            write_credentials(doc, &config.providers.antigravity.credentials);
            Ok(())
        })
        .await
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
        self.with_document_mut(|doc| {
            write_credentials(doc, &config.providers.antigravity.credentials);
            Ok(())
        })
        .await
    }

    async fn delete_credential(&self, project_id: &str) -> Result<()> {
        let _guard = self.lock_update().await;
        let config = self.update_cached_config(|config| {
            let provider = &mut config.providers.antigravity;
            provider
                .credentials
                .retain(|cred| cred.project_id != project_id);
            provider.rebuild_credential_index();
        });
        self.with_document_mut(|doc| {
            write_credentials(doc, &config.providers.antigravity.credentials);
            Ok(())
        })
        .await
    }

    async fn get_credential(&self, index: usize) -> Result<Option<AntigravityCredential>> {
        let config = self.get_app_config().await?;
        Ok(config.providers.antigravity.credentials.get(index).cloned())
    }
}

fn write_setting(doc: &mut DocumentMut, setting: &AntigravitySetting) {
    let setting_item =
        doc["providers"]["antigravity"]["setting"].or_insert(Item::Table(Table::new()));
    let setting_table = setting_item.as_table_mut().expect("setting table");
    setting_table["base_url"] = Value::from(setting.base_url.to_string()).into();
    setting_table["rotate_num"] = Value::from(setting.rotate_num as i64).into();
}

fn write_credentials(doc: &mut DocumentMut, credentials: &[AntigravityCredential]) {
    let mut array = ArrayOfTables::new();
    for credential in credentials {
        let mut table = Table::new();
        table["project_id"] = Value::from(credential.project_id.clone()).into();
        table["client_email"] = Value::from(credential.client_email.clone()).into();
        table["client_id"] = Value::from(credential.client_id.clone()).into();
        table["client_secret"] = Value::from(credential.client_secret.clone()).into();
        table["token"] = Value::from(credential.token.clone()).into();
        table["refresh_token"] = Value::from(credential.refresh_token.clone()).into();
        let mut scope = Array::new();
        for item in &credential.scope {
            scope.push(item.as_str());
        }
        table["scope"] = Item::Value(Value::Array(scope));
        table["token_uri"] = Value::from(credential.token_uri.clone()).into();
        table["expiry"] = Value::from(credential.expiry.clone()).into();
        if let Some(states) = states_to_item(&credential.states) {
            table["states"] = states;
        }
        array.push(table);
    }
    doc["providers"]["antigravity"]["credentials"] = Item::ArrayOfTables(array);
}

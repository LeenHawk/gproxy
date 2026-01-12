use anyhow::Result;
use async_trait::async_trait;
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, Value};

use crate::providers::credential_index;
use crate::providers::openai::{OpenAIBackend, OpenAICredential, OpenAISetting};
use crate::storage::{ConfigStore, S3Storage};
use crate::storage::toml_states::states_to_item;

#[async_trait]
impl OpenAIBackend for S3Storage {
    async fn get_config(&self) -> Result<OpenAISetting> {
        let config = self.get_app_config().await?;
        Ok(config.providers.openai.setting)
    }

    async fn load_config(&self) -> Result<OpenAISetting> {
        let config = self.load_app_config().await?;
        Ok(config.providers.openai.setting)
    }

    async fn update_config<F>(&self, update: F) -> Result<()>
    where
        F: FnOnce(&mut OpenAISetting) + Send,
    {
        let _guard = self.lock_update().await;
        let mut updated = None;
        let config = self.update_cached_config(|config| {
            update(&mut config.providers.openai.setting);
            updated = Some(config.providers.openai.setting.clone());
        });

        let setting = updated.unwrap_or(config.providers.openai.setting);
        self.with_document_mut(|doc| {
            write_setting(doc, &setting);
            Ok(())
        })
        .await
    }

    async fn get_credentials(&self) -> Result<Vec<OpenAICredential>> {
        let config = self.get_app_config().await?;
        Ok(config.providers.openai.credentials)
    }

    async fn load_credentials(&self) -> Result<Vec<OpenAICredential>> {
        let config = self.load_app_config().await?;
        Ok(config.providers.openai.credentials)
    }

    async fn add_credential(&self, credential: OpenAICredential) -> Result<()> {
        let _guard = self.lock_update().await;
        let config = self.update_cached_config(|config| {
            let provider = &mut config.providers.openai;
            credential_index::ensure_index(&mut provider.credential_index, &provider.credentials);
            let key = credential.key.trim().to_string();
            provider.credentials.push(credential);
            if !key.is_empty() {
                provider
                    .credential_index
                    .insert(key, provider.credentials.len() - 1);
            }
        });
        self.with_document_mut(|doc| {
            write_credentials(doc, &config.providers.openai.credentials);
            Ok(())
        })
        .await
    }

    async fn update_credential<F>(&self, index: usize, update: F) -> Result<()>
    where
        F: FnOnce(&mut OpenAICredential) + Send,
    {
        let _guard = self.lock_update().await;
        let config = self.update_cached_config(|config| {
            let provider = &mut config.providers.openai;
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
        self.with_document_mut(|doc| {
            write_credentials(doc, &config.providers.openai.credentials);
            Ok(())
        })
        .await
    }

    async fn update_credential_by_id<F>(&self, id: &str, update: F) -> Result<()>
    where
        F: FnOnce(&mut OpenAICredential) + Send,
    {
        let _guard = self.lock_update().await;
        let config = self.update_cached_config(|config| {
            let provider = &mut config.providers.openai;
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
        self.with_document_mut(|doc| {
            write_credentials(doc, &config.providers.openai.credentials);
            Ok(())
        })
        .await
    }

    async fn delete_credential(&self, key: &str) -> Result<()> {
        let _guard = self.lock_update().await;
        let config = self.update_cached_config(|config| {
            let provider = &mut config.providers.openai;
            provider.credentials.retain(|cred| cred.key != key);
            provider.rebuild_credential_index();
        });
        self.with_document_mut(|doc| {
            write_credentials(doc, &config.providers.openai.credentials);
            Ok(())
        })
        .await
    }

    async fn get_credential(&self, index: usize) -> Result<Option<OpenAICredential>> {
        let config = self.get_app_config().await?;
        Ok(config.providers.openai.credentials.get(index).cloned())
    }
}

fn write_setting(doc: &mut DocumentMut, setting: &OpenAISetting) {
    let setting_item = doc["providers"]["openai"]["setting"].or_insert(Item::Table(Table::new()));
    let setting_table = setting_item.as_table_mut().expect("setting table");
    setting_table["base_url"] = Value::from(setting.base_url.to_string()).into();
    setting_table["rotate_num"] = Value::from(setting.rotate_num as i64).into();
}

fn write_credentials(doc: &mut DocumentMut, credentials: &[OpenAICredential]) {
    let mut array = ArrayOfTables::new();
    for credential in credentials {
        let mut table = Table::new();
        table["key"] = Value::from(credential.key.clone()).into();
        if let Some(states) = states_to_item(&credential.states) {
            table["states"] = states;
        }
        array.push(table);
    }
    doc["providers"]["openai"]["credentials"] = Item::ArrayOfTables(array);
}

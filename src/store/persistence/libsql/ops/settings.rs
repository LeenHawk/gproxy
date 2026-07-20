use crate::store::persistence::records::{InstanceSettings, InstanceSettingsInput};
use crate::store::persistence::traits::SettingsPersistence;

use super::super::{LibsqlPersistence, settings, tokenize};

#[async_trait::async_trait(?Send)]
impl SettingsPersistence for LibsqlPersistence {
    async fn list_instance_settings(&self) -> anyhow::Result<Vec<InstanceSettings>> {
        settings::instance_settings::list(&self.client).await
    }
    async fn get_instance_settings(
        &self,
        instance_name: &str,
    ) -> anyhow::Result<Option<InstanceSettings>> {
        settings::instance_settings::get(&self.client, instance_name).await
    }
    async fn upsert_instance_settings(
        &self,
        input: InstanceSettingsInput,
    ) -> anyhow::Result<InstanceSettings> {
        settings::instance_settings::upsert(&self.client, input).await
    }
    async fn list_tokenizer_vocabs(&self) -> anyhow::Result<Vec<String>> {
        tokenize::tokenizer_vocabs::list(&self.client).await
    }
    async fn get_tokenizer_vocab(&self, name: &str) -> anyhow::Result<Option<Vec<u8>>> {
        tokenize::tokenizer_vocabs::get(&self.client, name).await
    }
    async fn put_tokenizer_vocab(&self, name: &str, bytes: &[u8]) -> anyhow::Result<()> {
        tokenize::tokenizer_vocabs::put(&self.client, name, bytes).await
    }
}

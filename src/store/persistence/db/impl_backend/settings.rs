use async_trait::async_trait;

use super::super::{DbPersistence, ops};
use crate::store::persistence::records::{InstanceSettings, InstanceSettingsInput};
use crate::store::persistence::traits::SettingsPersistence;

#[async_trait]
impl SettingsPersistence for DbPersistence {
    async fn list_instance_settings(&self) -> anyhow::Result<Vec<InstanceSettings>> {
        ops::settings::instance_settings::list(&self.conn).await
    }
    async fn get_instance_settings(
        &self,
        instance_name: &str,
    ) -> anyhow::Result<Option<InstanceSettings>> {
        ops::settings::instance_settings::get(&self.conn, instance_name).await
    }
    async fn upsert_instance_settings(
        &self,
        input: InstanceSettingsInput,
    ) -> anyhow::Result<InstanceSettings> {
        ops::settings::instance_settings::upsert(&self.conn, input).await
    }
    async fn list_tokenizer_vocabs(&self) -> anyhow::Result<Vec<String>> {
        ops::tokenize::tokenizer_vocabs::list(&self.conn).await
    }
    async fn get_tokenizer_vocab(&self, name: &str) -> anyhow::Result<Option<Vec<u8>>> {
        ops::tokenize::tokenizer_vocabs::get(&self.conn, name).await
    }
    async fn put_tokenizer_vocab(&self, name: &str, bytes: &[u8]) -> anyhow::Result<()> {
        ops::tokenize::tokenizer_vocabs::put(&self.conn, name, bytes).await
    }
}

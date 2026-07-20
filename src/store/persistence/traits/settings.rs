use crate::store::persistence::records::{InstanceSettings, InstanceSettingsInput};

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait SettingsPersistence {
    async fn list_instance_settings(&self) -> anyhow::Result<Vec<InstanceSettings>>;
    async fn get_instance_settings(
        &self,
        instance_name: &str,
    ) -> anyhow::Result<Option<InstanceSettings>>;
    async fn upsert_instance_settings(
        &self,
        input: InstanceSettingsInput,
    ) -> anyhow::Result<InstanceSettings>;

    async fn list_tokenizer_vocabs(&self) -> anyhow::Result<Vec<String>> {
        Ok(vec![])
    }
    async fn get_tokenizer_vocab(&self, _name: &str) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(None)
    }
    async fn put_tokenizer_vocab(&self, _name: &str, _bytes: &[u8]) -> anyhow::Result<()> {
        anyhow::bail!("tokenizer vocab storage unsupported by this backend")
    }
}

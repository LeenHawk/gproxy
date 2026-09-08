#[cfg(not(target_arch = "wasm32"))]
use gproxy_store::records::{
    DEFAULT_TOKENIZER_VOCAB, ENABLE_TOKENIZER_DOWNLOAD, ENABLE_TOKENIZER_VOCABS,
};
use gproxy_store::records::{
    ENABLE_USAGE, INSTANCE_NAME, SettingRecord, TRAFFIC_BLACKLIST, UPDATE_CHANNEL,
};

pub(crate) use super::runtime_overrides::RuntimeOverrides;

#[derive(Clone)]
pub(crate) struct EffectiveSettings {
    pub runtime: std::sync::Arc<gproxy_admin::dto::RuntimeSettingsStatusDto>,
    pub enable_usage: bool,
    #[cfg(not(target_arch = "wasm32"))]
    pub enable_tokenizer_vocabs: bool,
    #[cfg(not(target_arch = "wasm32"))]
    pub enable_tokenizer_download: bool,
    #[cfg(not(target_arch = "wasm32"))]
    pub default_tokenizer_vocab: Option<String>,
    pub instance_name: String,
    pub instance_id: u64,
    pub traffic_blacklist: gproxy_channel_api::TrafficBlacklistConfig,
    pub update_channel: Option<String>,
}

impl EffectiveSettings {
    pub(super) fn read(
        values: &[SettingRecord],
        runtime: &RuntimeOverrides,
    ) -> Result<Self, gproxy_store::StoreError> {
        Ok(Self {
            runtime: std::sync::Arc::new(
                runtime.status(gproxy_admin::runtime_settings::read(values)?),
            ),
            enable_usage: boolean(values, ENABLE_USAGE, true),
            #[cfg(not(target_arch = "wasm32"))]
            enable_tokenizer_vocabs: boolean(values, ENABLE_TOKENIZER_VOCABS, true),
            #[cfg(not(target_arch = "wasm32"))]
            enable_tokenizer_download: boolean(values, ENABLE_TOKENIZER_DOWNLOAD, false),
            #[cfg(not(target_arch = "wasm32"))]
            default_tokenizer_vocab: text(values, DEFAULT_TOKENIZER_VOCAB),
            instance_name: text(values, INSTANCE_NAME).unwrap_or_else(|| "default".into()),
            instance_id: runtime.instance_id,
            traffic_blacklist: traffic_blacklist(values),
            update_channel: text(values, UPDATE_CHANNEL),
        })
    }

    pub(crate) fn inherit_system_proxy(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        return self.runtime.effective.inherit_system_proxy;
        #[cfg(target_arch = "wasm32")]
        false
    }
}

pub(crate) fn effective_proxy(
    credential: Option<&str>,
    provider: Option<&str>,
    global: Option<&str>,
) -> Option<String> {
    credential.or(provider).or(global).map(str::to_owned)
}

fn boolean(values: &[SettingRecord], key: &str, default: bool) -> bool {
    values
        .iter()
        .find(|setting| setting.key == key)
        .and_then(|setting| setting.value.as_bool())
        .unwrap_or(default)
}

fn text(values: &[SettingRecord], key: &str) -> Option<String> {
    values
        .iter()
        .find(|setting| setting.key == key)?
        .value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn traffic_blacklist(values: &[SettingRecord]) -> gproxy_channel_api::TrafficBlacklistConfig {
    values
        .iter()
        .find(|setting| setting.key == TRAFFIC_BLACKLIST)
        .and_then(|setting| {
            gproxy_channel_api::TrafficBlacklistConfig::from_value(&setting.value).ok()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::effective_proxy;

    #[test]
    fn proxy_fallback_covers_credential_provider_global_and_direct() {
        assert_eq!(
            effective_proxy(
                Some("http://credential"),
                Some("http://provider"),
                Some("http://global")
            ),
            Some("http://credential".into())
        );
        assert_eq!(
            effective_proxy(None, Some("http://provider"), Some("http://global")),
            Some("http://provider".into())
        );
        assert_eq!(
            effective_proxy(None, None, Some("http://global")),
            Some("http://global".into())
        );
        assert_eq!(effective_proxy(None, None, None), None);
    }
}

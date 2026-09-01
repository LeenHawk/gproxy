#[cfg(not(target_arch = "wasm32"))]
use gproxy_store::records::{
    DEFAULT_TOKENIZER_VOCAB, ENABLE_TOKENIZER_DOWNLOAD, ENABLE_TOKENIZER_VOCABS,
    INHERIT_SYSTEM_PROXY,
};
use gproxy_store::records::{
    ENABLE_USAGE, FILE_UPLOAD_MAX_IN_FLIGHT, INSTANCE_NAME, PROXY, SettingRecord, TRAFFIC_BLACKLIST,
};

#[derive(Clone)]
pub(crate) struct RuntimeOverrides {
    pub global_proxy: Option<String>,
    pub max_attempts: u32,
    pub file_upload_max_in_flight: Option<usize>,
    pub instance_id: u64,
}

impl RuntimeOverrides {
    pub(crate) fn from_config(config: &crate::Config) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        return Self {
            global_proxy: config.native().upstream_proxy_url.clone(),
            max_attempts: config.native().max_attempts,
            file_upload_max_in_flight: config.native().file_upload_max_in_flight,
            instance_id: config.native().instance_id,
        };
        #[cfg(target_arch = "wasm32")]
        {
            let _ = config;
            Self {
                global_proxy: None,
                max_attempts: 6,
                file_upload_max_in_flight: None,
                instance_id: 0,
            }
        }
    }
}

#[derive(Clone)]
pub(crate) struct EffectiveSettings {
    pub proxy: Option<String>,
    pub enable_usage: bool,
    #[cfg(not(target_arch = "wasm32"))]
    pub enable_tokenizer_vocabs: bool,
    #[cfg(not(target_arch = "wasm32"))]
    pub enable_tokenizer_download: bool,
    #[cfg(not(target_arch = "wasm32"))]
    pub default_tokenizer_vocab: Option<String>,
    pub file_upload_max_in_flight: usize,
    pub instance_name: String,
    #[cfg(not(target_arch = "wasm32"))]
    pub inherit_system_proxy: bool,
    pub max_attempts: u32,
    pub instance_id: u64,
    pub traffic_blacklist: gproxy_channel_api::TrafficBlacklistConfig,
}

impl EffectiveSettings {
    pub(super) fn read(values: &[SettingRecord], runtime: &RuntimeOverrides) -> Self {
        Self {
            proxy: runtime.global_proxy.clone().or_else(|| text(values, PROXY)),
            enable_usage: boolean(values, ENABLE_USAGE, true),
            #[cfg(not(target_arch = "wasm32"))]
            enable_tokenizer_vocabs: boolean(values, ENABLE_TOKENIZER_VOCABS, true),
            #[cfg(not(target_arch = "wasm32"))]
            enable_tokenizer_download: boolean(values, ENABLE_TOKENIZER_DOWNLOAD, false),
            #[cfg(not(target_arch = "wasm32"))]
            default_tokenizer_vocab: text(values, DEFAULT_TOKENIZER_VOCAB),
            file_upload_max_in_flight: runtime.file_upload_max_in_flight.unwrap_or_else(|| {
                unsigned(values, FILE_UPLOAD_MAX_IN_FLIGHT)
                    .and_then(|value| value.try_into().ok())
                    .unwrap_or(0)
            }),
            instance_name: text(values, INSTANCE_NAME).unwrap_or_else(|| "default".into()),
            #[cfg(not(target_arch = "wasm32"))]
            inherit_system_proxy: boolean(values, INHERIT_SYSTEM_PROXY, false),
            max_attempts: runtime.max_attempts,
            instance_id: runtime.instance_id,
            traffic_blacklist: traffic_blacklist(values),
        }
    }

    pub(crate) fn inherit_system_proxy(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        return self.inherit_system_proxy;
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

fn unsigned(values: &[SettingRecord], key: &str) -> Option<u64> {
    values
        .iter()
        .find(|setting| setting.key == key)?
        .value
        .as_u64()
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

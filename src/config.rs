use rand::{Rng, distr::Alphanumeric};
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr};
use url::Url;

use crate::providers::ProvidersConfig;
use crate::storage::StorageConfig;

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub app: AppSection,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub providers: ProvidersConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AppSection {
    #[serde(default = "default_host")]
    pub host: IpAddr,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "generate_secret")]
    pub admin_key: String,
    #[serde(default = "generate_secrets")]
    pub api_keys: Vec<String>,
    #[serde(default)]
    pub proxy: Option<Url>,
}

impl Default for AppSection {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            admin_key: generate_secret(),
            api_keys: generate_secrets(),
            proxy: None,
        }
    }
}

fn default_host() -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))
}

fn default_port() -> u16 {
    8787
}

fn generate_secret() -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(16)
        .map(char::from)
        .collect()
}

fn generate_secrets() -> Vec<String> {
    vec![generate_secret()]
}

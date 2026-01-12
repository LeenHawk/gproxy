use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use url::Url;

use crate::providers::credential_index::{self, CredentialKey};
use crate::providers::credential_status::{CredentialStatusList, DEFAULT_MODEL_KEY, now_timestamp};

static DEEPSEEK_ROTATE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct DeepSeekProvider {
    #[serde(default = "default_deepseek_setting")]
    pub setting: DeepSeekSetting,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credentials: Vec<DeepSeekCredential>,
    #[serde(skip, default)]
    pub credential_index: HashMap<String, usize>,
}

impl DeepSeekProvider {
    pub fn rebuild_credential_index(&mut self) {
        credential_index::rebuild_index(&mut self.credential_index, &self.credentials);
    }

    pub fn find_credential_index(&mut self, key: &str) -> Option<usize> {
        credential_index::find_or_rebuild(&mut self.credential_index, &self.credentials, key)
    }

    pub fn pick_credential(&self) -> Option<&DeepSeekCredential> {
        let now = now_timestamp();
        let valid: Vec<&DeepSeekCredential> = self
            .credentials
            .iter()
            .filter(|item| {
                !item.key.trim().is_empty()
                    && item.states.is_ready_for(DEFAULT_MODEL_KEY, now)
            })
            .collect();
        if valid.is_empty() {
            return None;
        }
        if self.setting.rotate_num == 0 {
            return Some(valid[0]);
        }

        let count = DEEPSEEK_ROTATE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let span = self.setting.rotate_num as u64;
        let slot = count / span;
        let idx = (slot % valid.len() as u64) as usize;
        Some(valid[idx])
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct DeepSeekSetting {
    pub base_url: Url,
    #[serde(default = "default_rotate_num")]
    pub rotate_num: u32,
}

impl Default for DeepSeekSetting {
    fn default() -> Self {
        Self {
            base_url: "https://api.deepseek.com"
                .parse()
                .expect("valid deepseek base url"),
            rotate_num: default_rotate_num(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct DeepSeekCredential {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub states: CredentialStatusList,
}

impl CredentialKey for DeepSeekCredential {
    fn credential_key(&self) -> &str {
        self.key.as_str()
    }
}

fn default_deepseek_setting() -> DeepSeekSetting {
    DeepSeekSetting::default()
}

fn default_rotate_num() -> u32 {
    1
}

// storage integration lives in providers/deepseek/storage.rs

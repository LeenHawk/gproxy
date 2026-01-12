use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use url::Url;

use crate::providers::credential_index::{self, CredentialKey};
use crate::providers::credential_status::{CredentialStatusList, DEFAULT_MODEL_KEY, now_timestamp};

static NVIDIA_ROTATE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct NvidiaProvider {
    #[serde(default = "default_nvidia_setting")]
    pub setting: NvidiaSetting,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credentials: Vec<NvidiaCredential>,
    #[serde(skip, default)]
    pub credential_index: HashMap<String, usize>,
}

impl NvidiaProvider {
    pub fn rebuild_credential_index(&mut self) {
        credential_index::rebuild_index(&mut self.credential_index, &self.credentials);
    }

    pub fn find_credential_index(&mut self, key: &str) -> Option<usize> {
        credential_index::find_or_rebuild(&mut self.credential_index, &self.credentials, key)
    }

    pub fn pick_credential(&self) -> Option<&NvidiaCredential> {
        let now = now_timestamp();
        let valid: Vec<&NvidiaCredential> = self
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

        let count = NVIDIA_ROTATE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let span = self.setting.rotate_num as u64;
        let slot = count / span;
        let idx = (slot % valid.len() as u64) as usize;
        Some(valid[idx])
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct NvidiaSetting {
    pub base_url: Url,
    #[serde(default = "default_rotate_num")]
    pub rotate_num: u32,
}

impl Default for NvidiaSetting {
    fn default() -> Self {
        Self {
            base_url: "https://integrate.api.nvidia.com"
                .parse()
                .expect("valid nvidia base url"),
            rotate_num: default_rotate_num(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct NvidiaCredential {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub states: CredentialStatusList,
}

impl CredentialKey for NvidiaCredential {
    fn credential_key(&self) -> &str {
        self.key.as_str()
    }
}

fn default_nvidia_setting() -> NvidiaSetting {
    NvidiaSetting::default()
}

fn default_rotate_num() -> u32 {
    1
}

// storage integration lives in providers/nvidia/storage.rs

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use url::Url;

use crate::providers::credential_index::{self, CredentialKey};
use crate::providers::credential_status::{CredentialStatusList, DEFAULT_MODEL_KEY, now_timestamp};

static CODEX_ROTATE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct CodexProvider {
    #[serde(default = "default_codex_setting")]
    pub setting: CodexSetting,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credentials: Vec<CodexCredential>,
    #[serde(skip, default)]
    pub credential_index: HashMap<String, usize>,
}

impl CodexProvider {
    pub fn rebuild_credential_index(&mut self) {
        credential_index::rebuild_index(&mut self.credential_index, &self.credentials);
    }

    pub fn find_credential_index(&mut self, key: &str) -> Option<usize> {
        credential_index::find_or_rebuild(&mut self.credential_index, &self.credentials, key)
    }

    pub fn pick_credential(&self) -> Option<&CodexCredential> {
        let now = now_timestamp();
        let valid: Vec<&CodexCredential> = self
            .credentials
            .iter()
            .filter(|item| {
                !item.refresh_token.trim().is_empty()
                    && !item.account_id.trim().is_empty()
                    && !item.access_token.trim().is_empty()
                    && item.states.is_ready_for(DEFAULT_MODEL_KEY, now)
            })
            .collect();
        if valid.is_empty() {
            return None;
        }
        if self.setting.rotate_num == 0 {
            return Some(valid[0]);
        }

        let count = CODEX_ROTATE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let span = self.setting.rotate_num as u64;
        let slot = count / span;
        let idx = (slot % valid.len() as u64) as usize;
        Some(valid[idx])
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct CodexSetting {
    pub base_url: Url,
    #[serde(default = "default_rotate_num")]
    pub rotate_num: u32,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct CodexCredential {
    #[serde(default)]
    pub id_token: String,
    #[serde(default)]
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: String,
    #[serde(default)]
    pub account_id: String,
    #[serde(default)]
    pub last_refresh: i64,
    #[serde(default)]
    pub states: CredentialStatusList,
}

impl CredentialKey for CodexCredential {
    fn credential_key(&self) -> &str {
        self.refresh_token.as_str()
    }
}

impl Default for CodexSetting {
    fn default() -> Self {
        Self {
            base_url: "https://chatgpt.com/backend-api/codex"
                .parse()
                .expect("valid codex base url"),
            rotate_num: default_rotate_num(),
        }
    }
}

fn default_codex_setting() -> CodexSetting {
    CodexSetting::default()
}

fn default_rotate_num() -> u32 {
    1
}

// storage integration lives in providers/codex/storage.rs

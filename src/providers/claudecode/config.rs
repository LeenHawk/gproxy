use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use url::Url;

use crate::providers::credential_index::{self, CredentialKey};
use crate::providers::credential_status::{CredentialStatusList, DEFAULT_MODEL_KEY, now_timestamp};

static CLAUDE_CODE_ROTATE_COUNTER: AtomicU64 = AtomicU64::new(0);
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ClaudeCodeProvider {
    #[serde(default = "default_claude_code_setting")]
    pub setting: ClaudeCodeSetting,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credentials: Vec<ClaudeCodeCredential>,
    #[serde(skip, default)]
    pub credential_index: HashMap<String, usize>,
}

impl ClaudeCodeProvider {
    pub fn rebuild_credential_index(&mut self) {
        credential_index::rebuild_index(&mut self.credential_index, &self.credentials);
    }

    pub fn find_credential_index(&mut self, key: &str) -> Option<usize> {
        credential_index::find_or_rebuild(&mut self.credential_index, &self.credentials, key)
    }

    pub fn pick_credential(&self) -> Option<&ClaudeCodeCredential> {
        self.pick_credential_for_model(DEFAULT_MODEL_KEY)
    }

    pub fn pick_credential_for_model(&self, model: &str) -> Option<&ClaudeCodeCredential> {
        let now = now_timestamp();
        let model_key = claudecode_model_key(model);
        let valid: Vec<&ClaudeCodeCredential> = self
            .credentials
            .iter()
            .filter(|item| {
                !item.refresh_token.trim().is_empty()
                    && !item.access_token.trim().is_empty()
                    && item.states.is_ready_for(model_key, now)
            })
            .collect();
        if valid.is_empty() {
            return None;
        }
        if self.setting.rotate_num == 0 {
            return Some(valid[0]);
        }

        let count = CLAUDE_CODE_ROTATE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let span = self.setting.rotate_num as u64;
        let slot = count / span;
        let idx = (slot % valid.len() as u64) as usize;
        Some(valid[idx])
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct ClaudeCodeSetting {
    pub base_url: Url,
    #[serde(default = "default_console_base_url")]
    pub console_base_url: Url,
    #[serde(default = "default_rotate_num")]
    pub rotate_num: u32,
}

impl Default for ClaudeCodeSetting {
    fn default() -> Self {
        Self {
            base_url: "https://api.anthropic.com"
                .parse()
                .expect("valid claude code base url"),
            console_base_url: default_console_base_url(),
            rotate_num: default_rotate_num(),
        }
    }
}

fn default_claude_code_setting() -> ClaudeCodeSetting {
    ClaudeCodeSetting::default()
}

fn default_rotate_num() -> u32 {
    1
}

fn default_console_base_url() -> Url {
    "https://console.anthropic.com"
        .parse()
        .expect("valid claude console base url")
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ClaudeCodeCredential {
    #[serde(default)]
    pub session_key: String,
    #[serde(default)]
    pub refresh_token: String,
    #[serde(default)]
    pub access_token: String,
    #[serde(default)]
    pub expires_at: i64,
    #[serde(default)]
    pub states: CredentialStatusList,
}

impl CredentialKey for ClaudeCodeCredential {
    fn credential_key(&self) -> &str {
        self.refresh_token.as_str()
    }
}

fn claudecode_model_key(model: &str) -> &str {
    if model.to_ascii_lowercase().contains("sonnet") {
        "sonnet"
    } else {
        DEFAULT_MODEL_KEY
    }
}

// storage integration lives in providers/claudecode/storage.rs

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use url::Url;

use crate::providers::credential_index::{self, CredentialKey};
use crate::providers::credential_status::{CredentialStatusList, DEFAULT_MODEL_KEY, now_timestamp};

static GEMINICLI_ROTATE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct GeminiCliProvider {
    #[serde(default = "default_gemini_cli_setting")]
    pub setting: GeminiCliSetting,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credentials: Vec<GeminiCliCredential>,
    #[serde(skip, default)]
    pub credential_index: HashMap<String, usize>,
}

impl GeminiCliProvider {
    pub fn rebuild_credential_index(&mut self) {
        credential_index::rebuild_index(&mut self.credential_index, &self.credentials);
    }

    pub fn find_credential_index(&mut self, key: &str) -> Option<usize> {
        credential_index::find_or_rebuild(&mut self.credential_index, &self.credentials, key)
    }

    pub fn pick_credential(&self) -> Option<&GeminiCliCredential> {
        self.pick_credential_for_model(DEFAULT_MODEL_KEY)
    }

    pub fn pick_credential_for_model(&self, model: &str) -> Option<&GeminiCliCredential> {
        let now = now_timestamp();
        let valid: Vec<&GeminiCliCredential> = self
            .credentials
            .iter()
            .filter(|item| {
                (!item.token.trim().is_empty() || !item.refresh_token.trim().is_empty())
                    && item.states.is_ready_for(model, now)
            })
            .collect();
        if valid.is_empty() {
            return None;
        }
        if self.setting.rotate_num == 0 {
            return Some(valid[0]);
        }

        let count = GEMINICLI_ROTATE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let span = self.setting.rotate_num as u64;
        let slot = count / span;
        let idx = (slot % valid.len() as u64) as usize;
        Some(valid[idx])
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct GeminiCliSetting {
    pub base_url: Url,
    #[serde(default = "default_rotate_num")]
    pub rotate_num: u32,
}

impl Default for GeminiCliSetting {
    fn default() -> Self {
        Self {
            base_url: "https://cloudcode-pa.googleapis.com"
                .parse()
                .expect("valid gemini cli base url"),
            rotate_num: default_rotate_num(),
        }
    }
}

fn default_gemini_cli_setting() -> GeminiCliSetting {
    GeminiCliSetting::default()
}

fn default_rotate_num() -> u32 {
    1
}

// storage integration lives in providers/geminicli/storage.rs

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GeminiCliCredential {
    #[serde(default)]
    pub project_id: String,
    #[serde(default)]
    pub client_email: String,
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub client_secret: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub refresh_token: String,
    pub scope: Vec<String>,
    pub token_uri: String,
    #[serde(default)]
    pub expiry: String,
    #[serde(default)]
    pub states: CredentialStatusList,
}

impl CredentialKey for GeminiCliCredential {
    fn credential_key(&self) -> &str {
        self.project_id.as_str()
    }
}

impl Default for GeminiCliCredential {
    fn default() -> Self {
        Self {
            project_id: String::new(),
            client_email: String::new(),
            client_id: String::new(),
            client_secret: String::new(),
            token: String::new(),
            refresh_token: String::new(),
            scope: vec!["https://www.googleapis.com/auth/cloud-platform".to_string()],
            token_uri: "https://oauth2.googleapis.com/token".to_string(),
            expiry: String::new(),
            states: CredentialStatusList::default(),
        }
    }
}

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use url::Url;

use crate::providers::credential_index::{self, CredentialKey};
use crate::providers::credential_status::{CredentialStatusList, DEFAULT_MODEL_KEY, now_timestamp};

static VERTEX_ROTATE_COUNTER: AtomicU64 = AtomicU64::new(0);
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct VertexProvider {
    #[serde(default = "default_vertex_setting")]
    pub setting: VertexSetting,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credentials: Vec<VertexCredential>,
    #[serde(skip, default)]
    pub credential_index: HashMap<String, usize>,
}

impl VertexProvider {
    pub fn rebuild_credential_index(&mut self) {
        credential_index::rebuild_index(&mut self.credential_index, &self.credentials);
    }

    pub fn find_credential_index(&mut self, key: &str) -> Option<usize> {
        credential_index::find_or_rebuild(&mut self.credential_index, &self.credentials, key)
    }

    pub fn pick_credential(&self) -> Option<&VertexCredential> {
        let now = now_timestamp();
        let valid: Vec<&VertexCredential> = self
            .credentials
            .iter()
            .filter(|item| {
                !item.project_id.trim().is_empty()
                    && item.states.is_ready_for(DEFAULT_MODEL_KEY, now)
            })
            .collect();
        if valid.is_empty() {
            return None;
        }
        if self.setting.rotate_num == 0 {
            return Some(valid[0]);
        }

        let count = VERTEX_ROTATE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let span = self.setting.rotate_num as u64;
        let slot = count / span;
        let idx = (slot % valid.len() as u64) as usize;
        Some(valid[idx])
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct VertexSetting {
    pub base_url: Url,
    #[serde(default = "default_rotate_num")]
    pub rotate_num: u32,
}

impl Default for VertexSetting {
    fn default() -> Self {
        Self {
            base_url: "https://us-central1-aiplatform.googleapis.com"
                .parse()
                .expect("valid vertex base url"),
            rotate_num: default_rotate_num(),
        }
    }
}

fn default_vertex_setting() -> VertexSetting {
    VertexSetting::default()
}

fn default_rotate_num() -> u32 {
    1
}

// storage integration lives in providers/vertex/storage.rs

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct VertexCredential {
    #[serde(default)]
    pub r#type: String,
    #[serde(default)]
    pub project_id: String,
    #[serde(default)]
    pub private_key: String,
    #[serde(default)]
    pub client_email: String,
    #[serde(default)]
    pub client_id: String,
    pub auth_uri: String,
    pub token_uri: String,
    pub auth_provider_x509_cert_url: String,
    pub client_x509_cert_url: String,
    pub universe_domain: String,
    #[serde(default)]
    pub access_token: String,
    #[serde(default)]
    pub expires_at: i64,
    #[serde(default)]
    pub states: CredentialStatusList,
}

impl CredentialKey for VertexCredential {
    fn credential_key(&self) -> &str {
        self.project_id.as_str()
    }
}

impl Default for VertexCredential {
    fn default() -> Self {
        Self {
            r#type: String::new(),
            project_id: String::new(),
            private_key: String::new(),
            client_email: String::new(),
            client_id: String::new(),
            auth_uri: "https://accounts.google.com/o/oauth2/auth".to_string(),
            token_uri: "https://oauth2.googleapis.com/token".to_string(),
            auth_provider_x509_cert_url: "https://www.googleapis.com/oauth2/v1/certs".to_string(),
            client_x509_cert_url: String::new(),
            universe_domain: "googleapis.com".to_string(),
            access_token: String::new(),
            expires_at: 0,
            states: CredentialStatusList::default(),
        }
    }
}

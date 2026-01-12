use std::collections::HashMap;

use std::sync::Arc;
use tokio::sync::Mutex;

use crate::providers::credential_status::ProviderKind;
use crate::providers::usage::{UsageRecord, UsageViewRecord};

use super::UsageViewSpec;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct ViewAnchorKey {
    view_name: String,
    provider: ProviderKind,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct ViewKey {
    view_name: String,
    anchor_ts: i64,
    slot_start: i64,
    provider: ProviderKind,
    caller_api_key: Option<String>,
    provider_credential_id: Option<String>,
    model: Option<String>,
    upstream_model: Option<String>,
    format: Option<String>,
}

#[derive(Clone)]
pub struct UsageViewCache {
    default_anchor_ts: i64,
    anchors: Arc<Mutex<HashMap<ViewAnchorKey, i64>>>,
    views: Arc<Mutex<HashMap<ViewKey, UsageViewRecord>>>,
}

impl UsageViewCache {
    pub fn new(anchor_ts: i64) -> Self {
        Self {
            default_anchor_ts: anchor_ts,
            anchors: Arc::new(Mutex::new(HashMap::new())),
            views: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn set_anchor(
        &self,
        provider: ProviderKind,
        view_name: &str,
        anchor_ts: i64,
    ) {
        let key = ViewAnchorKey {
            view_name: view_name.to_string(),
            provider,
        };
        let mut guard = self.anchors.lock().await;
        let should_prune = guard.get(&key).copied() != Some(anchor_ts);
        guard.insert(
            ViewAnchorKey {
                view_name: view_name.to_string(),
                provider,
            },
            anchor_ts,
        );
        drop(guard);
        if should_prune {
            self.prune_views(provider, view_name).await;
        }
    }

    pub async fn apply_record(&self, spec: &UsageViewSpec, record: &UsageRecord) {
        let anchor_ts = self.anchor_for(record.provider, spec.name).await;
        let slot_start = slot_start(anchor_ts, spec.slot_secs, record.created_at);
        let key = ViewKey {
            view_name: spec.name.to_string(),
            anchor_ts,
            slot_start,
            provider: record.provider,
            caller_api_key: record.caller_api_key.clone(),
            provider_credential_id: record.provider_credential_id.clone(),
            model: record.model.clone(),
            upstream_model: record.upstream_model.clone(),
            format: record.format.clone(),
        };
        let mut guard = self.views.lock().await;
        let entry = guard.entry(key).or_insert_with(|| UsageViewRecord {
            view_name: spec.name.to_string(),
            anchor_ts,
            slot_start,
            provider: record.provider,
            caller_api_key: record.caller_api_key.clone(),
            provider_credential_id: record.provider_credential_id.clone(),
            model: record.model.clone(),
            upstream_model: record.upstream_model.clone(),
            format: record.format.clone(),
            record_count: 0,
            oa_resp_input_tokens_sum: 0,
            oa_resp_output_tokens_sum: 0,
            oa_resp_total_tokens_sum: 0,
            oa_resp_input_tokens_details_cached_tokens_sum: 0,
            oa_resp_output_tokens_details_reasoning_tokens_sum: 0,
            oa_resp_output_tokens_details_audio_tokens_sum: 0,
            oa_resp_output_tokens_details_cached_tokens_sum: 0,
            oa_chat_prompt_tokens_sum: 0,
            oa_chat_completion_tokens_sum: 0,
            oa_chat_total_tokens_sum: 0,
            oa_chat_prompt_tokens_details_cached_tokens_sum: 0,
            oa_chat_completion_tokens_details_reasoning_tokens_sum: 0,
            oa_chat_completion_tokens_details_audio_tokens_sum: 0,
            claude_input_tokens_sum: 0,
            claude_output_tokens_sum: 0,
            claude_cache_creation_input_tokens_sum: 0,
            claude_cache_read_input_tokens_sum: 0,
            gemini_prompt_token_count_sum: 0,
            gemini_candidates_token_count_sum: 0,
            gemini_total_token_count_sum: 0,
            gemini_cached_content_token_count_sum: 0,
        });
        entry.record_count += 1;
        add_opt(&mut entry.oa_resp_input_tokens_sum, record.oa_resp_input_tokens);
        add_opt(&mut entry.oa_resp_output_tokens_sum, record.oa_resp_output_tokens);
        add_opt(&mut entry.oa_resp_total_tokens_sum, record.oa_resp_total_tokens);
        add_opt(
            &mut entry.oa_resp_input_tokens_details_cached_tokens_sum,
            record.oa_resp_input_tokens_details_cached_tokens,
        );
        add_opt(
            &mut entry.oa_resp_output_tokens_details_reasoning_tokens_sum,
            record.oa_resp_output_tokens_details_reasoning_tokens,
        );
        add_opt(
            &mut entry.oa_resp_output_tokens_details_audio_tokens_sum,
            record.oa_resp_output_tokens_details_audio_tokens,
        );
        add_opt(
            &mut entry.oa_resp_output_tokens_details_cached_tokens_sum,
            record.oa_resp_output_tokens_details_cached_tokens,
        );
        add_opt(&mut entry.oa_chat_prompt_tokens_sum, record.oa_chat_prompt_tokens);
        add_opt(
            &mut entry.oa_chat_completion_tokens_sum,
            record.oa_chat_completion_tokens,
        );
        add_opt(&mut entry.oa_chat_total_tokens_sum, record.oa_chat_total_tokens);
        add_opt(
            &mut entry.oa_chat_prompt_tokens_details_cached_tokens_sum,
            record.oa_chat_prompt_tokens_details_cached_tokens,
        );
        add_opt(
            &mut entry.oa_chat_completion_tokens_details_reasoning_tokens_sum,
            record.oa_chat_completion_tokens_details_reasoning_tokens,
        );
        add_opt(
            &mut entry.oa_chat_completion_tokens_details_audio_tokens_sum,
            record.oa_chat_completion_tokens_details_audio_tokens,
        );
        add_opt(&mut entry.claude_input_tokens_sum, record.claude_input_tokens);
        add_opt(&mut entry.claude_output_tokens_sum, record.claude_output_tokens);
        add_opt(
            &mut entry.claude_cache_creation_input_tokens_sum,
            record.claude_cache_creation_input_tokens,
        );
        add_opt(
            &mut entry.claude_cache_read_input_tokens_sum,
            record.claude_cache_read_input_tokens,
        );
        add_opt(
            &mut entry.gemini_prompt_token_count_sum,
            record.gemini_prompt_token_count,
        );
        add_opt(
            &mut entry.gemini_candidates_token_count_sum,
            record.gemini_candidates_token_count,
        );
        add_opt(
            &mut entry.gemini_total_token_count_sum,
            record.gemini_total_token_count,
        );
        add_opt(
            &mut entry.gemini_cached_content_token_count_sum,
            record.gemini_cached_content_token_count,
        );
    }

    pub async fn apply_record_all(&self, specs: &[UsageViewSpec], record: &UsageRecord) {
        for spec in specs {
            self.apply_record(spec, record).await;
        }
    }

    pub async fn insert_view_record(&self, record: UsageViewRecord) {
        {
            let mut guard = self.anchors.lock().await;
            guard.insert(
                ViewAnchorKey {
                    view_name: record.view_name.clone(),
                    provider: record.provider,
                },
                record.anchor_ts,
            );
        }
        let key = ViewKey {
            view_name: record.view_name.clone(),
            anchor_ts: record.anchor_ts,
            slot_start: record.slot_start,
            provider: record.provider,
            caller_api_key: record.caller_api_key.clone(),
            provider_credential_id: record.provider_credential_id.clone(),
            model: record.model.clone(),
            upstream_model: record.upstream_model.clone(),
            format: record.format.clone(),
        };
        let mut guard = self.views.lock().await;
        guard.insert(key, record);
    }

    pub async fn snapshot(&self) -> Vec<UsageViewRecord> {
        let guard = self.views.lock().await;
        guard.values().cloned().collect()
    }

    pub async fn query_by_api_key(&self, api_key: &str) -> Vec<UsageViewRecord> {
        let guard = self.views.lock().await;
        guard
            .values()
            .filter(|item| item.caller_api_key.as_deref() == Some(api_key))
            .cloned()
            .collect()
    }

    pub async fn query_by_provider_credential(
        &self,
        provider: ProviderKind,
        credential_id: &str,
    ) -> Vec<UsageViewRecord> {
        let guard = self.views.lock().await;
        guard
            .values()
            .filter(|item| {
                item.provider == provider
                    && item.provider_credential_id.as_deref() == Some(credential_id)
            })
            .cloned()
            .collect()
    }

    async fn anchor_for(&self, provider: ProviderKind, view_name: &str) -> i64 {
        let guard = self.anchors.lock().await;
        guard
            .get(&ViewAnchorKey {
                view_name: view_name.to_string(),
                provider,
            })
            .copied()
            .unwrap_or(self.default_anchor_ts)
    }

    async fn prune_views(&self, provider: ProviderKind, view_name: &str) {
        let mut guard = self.views.lock().await;
        guard.retain(|key, _| !(key.provider == provider && key.view_name == view_name));
    }
}

fn slot_start(anchor_ts: i64, slot_secs: i64, created_at: i64) -> i64 {
    if slot_secs <= 0 {
        return anchor_ts;
    }
    let offset = created_at - anchor_ts;
    let slot_id = offset.div_euclid(slot_secs);
    anchor_ts + slot_id * slot_secs
}

fn add_opt(target: &mut i64, value: Option<i64>) {
    if let Some(value) = value {
        *target += value;
    }
}

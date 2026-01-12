use async_trait::async_trait;
use axum::http::StatusCode;
use serde_json::Value;

use crate::context::AppContext;
use crate::providers::admin::ProviderAdmin;
use crate::providers::codex::{
    CodexCredential, CodexSetting, cooldown_until_from_usage, fetch_codex_usage,
};
use crate::providers::credential_status::{CredentialStatus, ProviderKind, now_timestamp, DEFAULT_MODEL_KEY};
use crate::usage::{next_anchor_ts, slot_secs_for_view};

pub struct CodexAdmin;

#[async_trait]
impl ProviderAdmin for CodexAdmin {
    async fn get_config(&self, ctx: &AppContext) -> Result<Value, StatusCode> {
        let storage = ctx.codex();
        let config = storage
            .get_config()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        serde_json::to_value(config).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
    }

    async fn put_config(&self, ctx: &AppContext, config: Value) -> Result<Value, StatusCode> {
        let next: CodexSetting =
            serde_json::from_value(config).map_err(|_| StatusCode::BAD_REQUEST)?;
        let storage = ctx.codex();
        let saved = next.clone();
        storage
            .update_config(move |setting| {
                *setting = next;
            })
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        serde_json::to_value(saved).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
    }

    async fn list_credentials(&self, ctx: &AppContext) -> Result<Value, StatusCode> {
        let storage = ctx.codex();
        let credentials = storage
            .get_credentials()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        serde_json::to_value(credentials).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
    }

    async fn add_credential(&self, ctx: &AppContext, credential: Value) -> Result<(), StatusCode> {
        let mut credential: CodexCredential =
            serde_json::from_value(credential).map_err(|_| StatusCode::BAD_REQUEST)?;
        if let Ok(usage) = fetch_codex_usage(ctx, &credential).await {
            let primary_start = usage.rate_limit.primary_window.start_ts();
            let secondary_start = usage
                .rate_limit
                .secondary_window
                .as_ref()
                .map(|window| window.start_ts())
                .unwrap_or(primary_start);
            ctx
                .usage_store()
                .set_anchor(ProviderKind::Codex, "5h", primary_start)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            ctx
                .usage_store()
                .set_anchor(ProviderKind::Codex, "1w", secondary_start)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let now = now_timestamp();
            for (view_name, anchor_ts) in [("5h", primary_start), ("1w", secondary_start)] {
                if let Some(slot_secs) = slot_secs_for_view(ProviderKind::Codex, view_name)
                    && slot_secs > 0
                {
                    let next_until = next_anchor_ts(anchor_ts, slot_secs, now);
                    ctx.schedule_usage_anchor(ProviderKind::Codex, view_name, next_until)
                        .await;
                }
            }
            if usage.rate_limit.limit_reached || !usage.rate_limit.allowed {
                let until = cooldown_until_from_usage(&usage);
            credential
                .states
                .update_status(DEFAULT_MODEL_KEY, CredentialStatus::Cooldown { until });
            }
        }
        let storage = ctx.codex();
        storage
            .add_credential(credential)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(())
    }

    async fn update_credential(
        &self,
        ctx: &AppContext,
        index: usize,
        credential: Value,
    ) -> Result<(), StatusCode> {
        let credential: CodexCredential =
            serde_json::from_value(credential).map_err(|_| StatusCode::BAD_REQUEST)?;
        let storage = ctx.codex();
        let exists = storage
            .get_credential(index)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if exists.is_none() {
            return Err(StatusCode::NOT_FOUND);
        }
        storage
            .update_credential(index, move |stored| {
                *stored = credential;
            })
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(())
    }

    async fn delete_credential(&self, ctx: &AppContext, index: usize) -> Result<(), StatusCode> {
        let storage = ctx.codex();
        let credential = storage
            .get_credential(index)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let Some(credential) = credential else {
            return Err(StatusCode::NOT_FOUND);
        };
        storage
            .delete_credential(&credential.refresh_token)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(())
    }
}

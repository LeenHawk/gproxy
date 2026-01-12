use async_trait::async_trait;
use axum::http::StatusCode;
use serde_json::Value;

use crate::context::AppContext;
use crate::providers::admin::ProviderAdmin;
use crate::providers::credential_status::{ProviderKind, now_timestamp};
use crate::providers::claudecode::{
    ClaudeCodeCredential, ClaudeCodeSetting, exchange_session_key,
};
use crate::usage::{DEFAULT_USAGE_VIEWS, next_anchor_ts, set_default_usage_anchors};

pub struct ClaudeCodeAdmin;

#[async_trait]
impl ProviderAdmin for ClaudeCodeAdmin {
    async fn get_config(&self, ctx: &AppContext) -> Result<Value, StatusCode> {
        let storage = ctx.claudecode();
        let config = storage
            .get_config()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        serde_json::to_value(config).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
    }

    async fn put_config(&self, ctx: &AppContext, config: Value) -> Result<Value, StatusCode> {
        let next: ClaudeCodeSetting =
            serde_json::from_value(config).map_err(|_| StatusCode::BAD_REQUEST)?;
        let storage = ctx.claudecode();
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
        let storage = ctx.claudecode();
        let credentials = storage
            .get_credentials()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        serde_json::to_value(credentials).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
    }

    async fn add_credential(&self, ctx: &AppContext, credential: Value) -> Result<(), StatusCode> {
        let mut credential: ClaudeCodeCredential =
            serde_json::from_value(credential).map_err(|_| StatusCode::BAD_REQUEST)?;
        let now = now_timestamp();
        let expired = credential.expires_at > 0 && credential.expires_at <= now;
        if (credential.refresh_token.trim().is_empty()
            || credential.access_token.trim().is_empty()
            || expired)
            && !credential.session_key.trim().is_empty()
        {
            let storage = ctx.claudecode();
            let setting = storage
                .get_config()
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let tokens = exchange_session_key(
                ctx,
                &credential,
                &setting.base_url,
            )
            .await?;
            credential.refresh_token = tokens.refresh_token;
            credential.access_token = tokens.access_token;
            credential.expires_at = tokens.expires_at;
        }
        let storage = ctx.claudecode();
        storage
            .add_credential(credential)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let now = now_timestamp();
        let _ = set_default_usage_anchors(
            ctx.usage_store().as_ref(),
            ProviderKind::ClaudeCode,
            now,
        )
        .await;
        for spec in DEFAULT_USAGE_VIEWS {
            if spec.slot_secs <= 0 {
                continue;
            }
            let next_until = next_anchor_ts(now, spec.slot_secs, now);
            ctx.schedule_usage_anchor(ProviderKind::ClaudeCode, spec.name, next_until)
                .await;
        }
        Ok(())
    }

    async fn update_credential(
        &self,
        ctx: &AppContext,
        index: usize,
        credential: Value,
    ) -> Result<(), StatusCode> {
        let mut credential: ClaudeCodeCredential =
            serde_json::from_value(credential).map_err(|_| StatusCode::BAD_REQUEST)?;
        let storage = ctx.claudecode();
        let exists = storage
            .get_credential(index)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if exists.is_none() {
            return Err(StatusCode::NOT_FOUND);
        }
        let now = now_timestamp();
        let expired = credential.expires_at > 0 && credential.expires_at <= now;
        if (credential.refresh_token.trim().is_empty()
            || credential.access_token.trim().is_empty()
            || expired)
            && !credential.session_key.trim().is_empty()
        {
            let setting = storage
                .get_config()
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let tokens = exchange_session_key(ctx, &credential, &setting.base_url).await?;
            credential.refresh_token = tokens.refresh_token;
            credential.access_token = tokens.access_token;
            credential.expires_at = tokens.expires_at;
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
        let storage = ctx.claudecode();
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

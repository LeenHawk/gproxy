//! GLM platform and Coding Plan. Wire sources: upstream_docs/glm/docs/.
//! https://docs.bigmodel.cn/cn/guide/develop/http/introduction
//! https://docs.bigmodel.cn/cn/coding-plan/tool/others
//! https://docs.z.ai/devpack/tool/others (checked 2026-10-03).
//! Separate `glm` and `glmcode` products share their wire mechanics, but not
//! their endpoints, model catalogue templates or quota capability.

mod models;
mod quota;

use crate::channel::{
    BaseChannel, ChannelCapabilities, ChannelDescriptor, ChannelError, ConfigKey, ConfigKeyKind,
    HOST_CONFIG_KEYS, HeaderAllowlist, LoginMode, OperationContext, OperationFuture,
    PrepareContext, ProviderView, QuotaQuery, forwardable,
};
use crate::channels::shared::compatible::http::{insert_configured, strip_query_auth};
use gproxy_protocol::{Dialect, HttpBody, Operation, WireResponse};
use http::{HeaderValue, header};
use serde::Deserialize;
use std::collections::BTreeMap;

pub const ID: &str = "glm";
pub const CODING_ID: &str = "glmcode";
pub const DEFAULT_BASE_URL: &str = "https://open.bigmodel.cn";

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct GlmConfig {
    pub headers: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy)]
pub struct Glm {
    coding_plan: bool,
}

impl Glm {
    pub const API: Self = Self { coding_plan: false };
    pub const CODING: Self = Self { coding_plan: true };
}

impl BaseChannel for Glm {
    fn id(&self) -> &'static str {
        if self.coding_plan { CODING_ID } else { ID }
    }

    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: self.id(),
            display_name: if self.coding_plan { "GLM Coding Plan / Z.ai" } else { "GLM API / Z.ai" },
            login_modes: vec![LoginMode::ApiKey],
            capabilities: ChannelCapabilities { quota_query: self.coding_plan, ..Default::default() },
            config_keys: [
                ConfigKey::optional("base_url", ConfigKeyKind::String,
                    "Upstream origin: https://open.bigmodel.cn (China) or https://api.z.ai (global).")
                    .with_placeholder(DEFAULT_BASE_URL),
                ConfigKey::optional("headers", ConfigKeyKind::HeaderList, "Static upstream headers."),
            ].into_iter().chain(HOST_CONFIG_KEYS).collect(),
        }
    }

    fn native_dialects(&self, _: ProviderView<'_>, operation: Operation) -> Vec<Dialect> {
        if matches!(operation, Operation::ListModels | Operation::GetModel) {
            return vec![Dialect::OpenAi];
        }
        if !matches!(
            operation,
            Operation::GenerateContent | Operation::StreamGenerateContent
        ) {
            return vec![];
        }
        if self.coding_plan {
            vec![Dialect::OpenAiChat, Dialect::Claude, Dialect::OpenAi]
        } else {
            vec![Dialect::OpenAiChat]
        }
    }

    fn list_models<'a>(
        &'a self,
        ctx: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(models::reply(self, ctx, false))
    }
    fn get_model<'a>(
        &'a self,
        ctx: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(models::reply(self, ctx, true))
    }
    fn quota_query(&self) -> Option<&dyn QuotaQuery> {
        self.coding_plan.then_some(self as &dyn QuotaQuery)
    }

    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        let config: GlmConfig = serde_json::from_value(ctx.provider.config.clone())
            .map_err(|e| ChannelError::InvalidConfig(e.to_string()))?;
        if matches!(
            ctx.operation.operation,
            Operation::ListModels | Operation::GetModel
        ) {
            return Err(ChannelError::UnsupportedOperation(ctx.operation));
        }
        if !self
            .native_dialects(ctx.provider, ctx.operation.operation)
            .contains(&ctx.operation.dialect)
        {
            return Err(ChannelError::UnsupportedOperation(ctx.operation));
        }
        let key = ctx
            .credential
            .secret
            .get("api_key")
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .ok_or(ChannelError::InvalidCredential)?;
        let path = match ctx.operation.dialect {
            Dialect::Claude => "/api/anthropic/v1/messages",
            Dialect::OpenAi => "/api/v1/responses",
            _ if self.coding_plan => "/api/coding/paas/v4/chat/completions",
            _ => "/api/paas/v4/chat/completions",
        };
        let base = ctx
            .provider
            .base_url
            .unwrap_or(DEFAULT_BASE_URL)
            .trim_end_matches('/');
        let mut url = ctx
            .endpoint_override
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{base}{path}"));
        if let Some(query) = ctx
            .request
            .query
            .as_deref()
            .map(strip_query_auth)
            .filter(|q| !q.is_empty())
        {
            url.push(if url.contains('?') { '&' } else { '?' });
            url.push_str(&query);
        }
        let allowlist = HeaderAllowlist::from_view_for(
            ctx.provider,
            crate::channel::ChannelHeaders::native(ctx.operation.dialect),
        )?;
        let mut headers = forwardable(&ctx.request.headers, allowlist.as_ref(), &[]);
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {key}"))
                .map_err(|_| ChannelError::InvalidCredential)?,
        );
        for (name, value) in &config.headers {
            insert_configured(&mut headers, name, value)?;
        }
        let mut builder = http::Request::builder().method(ctx.request.method).uri(url);
        *builder.headers_mut().expect("fresh builder") = headers;
        builder
            .body(ctx.request.body)
            .map_err(|e| ChannelError::InvalidConfig(e.to_string()))
    }
}

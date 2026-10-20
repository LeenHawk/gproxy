//! Ark Agent Plan and Coding Plan, checked against the official API on 2026-10-20.
//! Wire sources: https://docs.volcengine.com/docs/ark/
//! - agent-plan-personal-other-tools / coding-plan-personal-ai-other-tools
//! - agent-plan-personal-codex / coding-plan-personal-ai-codex (Responses)
//! - list-ark-agent-plan-model-api / list-ark-coding-plan-model-api
//! - agent-plan-personal-vectorized-models / agent-plan-personal-visual-models
//! - agent-plan-personal-voice-model (the separate speech origin)
//!
//! Submodules name their control-plane and media contracts. Inference uses
//! the product's API key; discovery and quota use optional IAM AK/SK credentials.

mod audio;
mod control;
mod models;
mod quota;
mod request;
mod services;
mod usage;
mod video;

use crate::channel::{
    BaseChannel, ChannelCapabilities, ChannelDescriptor, ChannelError, ChannelServices, ConfigKey,
    ConfigKeyKind, HOST_CONFIG_KEYS, LoginMode, OperationContext, OperationFuture, PrepareContext,
    ProviderView, QuotaModel, QuotaQuery, UsageExtras,
};
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireResponse};
use serde::Deserialize;
use std::collections::BTreeMap;

pub const DEFAULT_BASE_URL: &str = "https://ark.cn-beijing.volces.com";
pub const CONTROL_BASE_URL: &str = "https://ark.cn-beijing.volcengineapi.com";
pub const SPEECH_BASE_URL: &str = "https://openspeech.bytedance.com";

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Edition {
    #[default]
    Personal,
    Enterprise,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
struct Config {
    edition: Edition,
    control_base_url: String,
    speech_base_url: String,
    headers: BTreeMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            edition: Edition::Personal,
            control_base_url: CONTROL_BASE_URL.into(),
            speech_base_url: SPEECH_BASE_URL.into(),
            headers: BTreeMap::new(),
        }
    }
}

impl Config {
    fn read(provider: ProviderView<'_>) -> Result<Self, ChannelError> {
        serde_json::from_value(provider.config.clone())
            .map_err(|e| ChannelError::InvalidConfig(e.to_string()))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Volcengine {
    agent: bool,
}

impl Volcengine {
    pub const AGENT: Self = Self { agent: true };
    pub const CODING: Self = Self { agent: false };

    fn prefix(&self) -> &'static str {
        if self.agent {
            "/api/plan"
        } else {
            "/api/coding"
        }
    }
}

impl BaseChannel for Volcengine {
    fn id(&self) -> &'static str {
        if self.agent {
            "volcengine_agent"
        } else {
            "volcengine_coding"
        }
    }

    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: self.id(),
            display_name: if self.agent { "Volcengine Agent Plan" } else { "Volcengine Coding Plan" },
            credential_kinds: vec!["api_key"],
            login_modes: vec![LoginMode::ApiKey],
            capabilities: ChannelCapabilities {
                quota_query: true,
                services: true,
                paid_usage: self.agent,
                ..Default::default()
            },
            config_keys: [
                ConfigKey::optional("base_url", ConfigKeyKind::String,
                    "Ark inference origin, without /api/plan or /api/coding. Agent Plan requires its dedicated API key.")
                    .with_placeholder(DEFAULT_BASE_URL),
                ConfigKey::optional("edition", ConfigKeyKind::String,
                    "Plan edition: personal (default) or enterprise. Enterprise quota requires credential metadata.seat_id."),
                ConfigKey::optional("control_base_url", ConfigKeyKind::String,
                    "Ark control-plane origin. Model discovery, plan information and quota require access_key_id and secret_access_key in the credential secret.")
                    .with_placeholder(CONTROL_BASE_URL),
                ConfigKey::optional("speech_base_url", ConfigKeyKind::String,
                    "Agent Plan speech origin for HTTP TTS.").with_placeholder(SPEECH_BASE_URL),
                ConfigKey::optional("headers", ConfigKeyKind::HeaderList, "Static upstream headers."),
            ].into_iter().chain(HOST_CONFIG_KEYS).collect(),
        }
    }

    fn native_dialects(&self, _: ProviderView<'_>, operation: Operation) -> Vec<Dialect> {
        match operation {
            Operation::GenerateContent | Operation::StreamGenerateContent => {
                vec![Dialect::OpenAiChat, Dialect::Claude, Dialect::OpenAi]
            }
            Operation::ListModels | Operation::GetModel | Operation::CreateEmbedding => {
                vec![Dialect::OpenAi]
            }
            Operation::CreateImage
            | Operation::CreateSpeech
            | Operation::CreateVideo
            | Operation::RetrieveVideo
            | Operation::ListVideos
            | Operation::DeleteVideo
            | Operation::DownloadVideoContent
                if self.agent =>
            {
                vec![Dialect::OpenAi]
            }
            _ => vec![],
        }
    }

    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        request::prepare(self, ctx)
    }

    fn default_conversion_target(
        &self,
        provider: ProviderView<'_>,
        source: OperationKey,
    ) -> Option<OperationKey> {
        if !matches!(
            source.operation,
            Operation::GenerateContent | Operation::StreamGenerateContent
        ) || self
            .native_dialects(provider, source.operation)
            .contains(&source.dialect)
        {
            return None;
        }
        Some(if source.dialect == Dialect::OpenAiResponsesWebSocket {
            OperationKey {
                operation: Operation::StreamGenerateContent,
                dialect: Dialect::OpenAi,
            }
        } else {
            OperationKey {
                operation: source.operation,
                dialect: Dialect::OpenAiChat,
            }
        })
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
    fn create_video<'a>(
        &'a self,
        ctx: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(video::call(self, Operation::CreateVideo, ctx))
    }
    fn retrieve_video<'a>(
        &'a self,
        ctx: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(video::call(self, Operation::RetrieveVideo, ctx))
    }
    fn list_videos<'a>(
        &'a self,
        ctx: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(video::call(self, Operation::ListVideos, ctx))
    }
    fn delete_video<'a>(
        &'a self,
        ctx: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(video::call(self, Operation::DeleteVideo, ctx))
    }
    fn download_video_content<'a>(
        &'a self,
        ctx: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(video::download(self, ctx))
    }
    fn create_speech<'a>(
        &'a self,
        ctx: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(audio::call(self, ctx))
    }
    fn quota_query(&self) -> Option<&dyn QuotaQuery> {
        Some(self)
    }
    fn quota_model(&self) -> Option<&dyn QuotaModel> {
        Some(self)
    }
    fn services(&self) -> Option<&dyn ChannelServices> {
        Some(self)
    }
    fn usage_extras(&self) -> Option<&dyn UsageExtras> {
        Some(self)
    }
}

fn json_response(status: http::StatusCode, body: serde_json::Value) -> WireResponse<HttpBody> {
    let mut headers = http::HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/json"),
    );
    WireResponse {
        status,
        headers,
        body: HttpBody::Bytes(body.to_string().into()),
    }
}

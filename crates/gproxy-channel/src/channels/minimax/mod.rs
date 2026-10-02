//! MiniMax text / Token Plan and H3 video V2.
//! Official wire contracts are saved in upstream_docs/minimax/docs/.
//! https://platform.minimax.io/docs/api-reference/text-anthropic-api
//! https://platform.minimax.io/docs/api-reference/text-openai-api
//! https://platform.minimax.io/docs/api-reference/video-generation-v2-create
//! https://platform.minimax.io/docs/token-plan/quickstart (checked 2026-10-03).
//! Text accepts both subscription and pay-as-you-go keys at the same URL.
//! H3 video requires a pay-as-you-go key. No credential switching is done here.

mod quota;
mod request;
mod video;

use crate::channel::{
    BaseChannel, ChannelCapabilities, ChannelDescriptor, ChannelError, ConfigKey, ConfigKeyKind,
    HOST_CONFIG_KEYS, LoginMode, OperationContext, OperationFuture, PrepareContext, ProviderView,
    QuotaQuery,
};
use gproxy_protocol::{Dialect, HttpBody, Operation, WireResponse};
use serde::Deserialize;
use std::collections::BTreeMap;

pub const ID: &str = "minimax";
pub const DEFAULT_BASE_URL: &str = "https://api.minimax.io";

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct MiniMaxConfig {
    pub headers: BTreeMap<String, String>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct MiniMax;

impl BaseChannel for MiniMax {
    fn id(&self) -> &'static str {
        ID
    }

    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: ID,
            display_name: "MiniMax",
            login_modes: vec![LoginMode::ApiKey],
            capabilities: ChannelCapabilities { quota_query: true, ..Default::default() },
            config_keys: [
                ConfigKey::optional("base_url", ConfigKeyKind::String,
                    "MiniMax API origin. Text accepts a Subscription Key or API Key; H3 video requires a pay-as-you-go API Key.")
                    .with_placeholder(DEFAULT_BASE_URL),
                ConfigKey::optional("headers", ConfigKeyKind::HeaderList, "Static upstream headers."),
            ].into_iter().chain(HOST_CONFIG_KEYS).collect(),
        }
    }

    fn native_dialects(&self, _: ProviderView<'_>, operation: Operation) -> Vec<Dialect> {
        match operation {
            Operation::ListModels | Operation::GetModel => vec![Dialect::OpenAi],
            Operation::GenerateContent | Operation::StreamGenerateContent => {
                vec![Dialect::OpenAiChat, Dialect::Claude]
            }
            Operation::CreateVideo
            | Operation::RetrieveVideo
            | Operation::ListVideos
            | Operation::DeleteVideo
            | Operation::DownloadVideoContent => vec![Dialect::OpenAi],
            _ => vec![],
        }
    }

    fn quota_query(&self) -> Option<&dyn QuotaQuery> {
        Some(self)
    }

    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        request::prepare(ctx)
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
}

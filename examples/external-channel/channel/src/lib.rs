use std::sync::Arc;

use gproxy_channel_api::protocol::{
    ContentGenerationKind::{OpenAiChatCompletions, OpenAiResponses},
    Operation::{GenerateContent, GetModel, ListModels, StreamGenerateContent},
    Provider,
};
use gproxy_channel_api::routes::{self, RouteList};
use gproxy_channel_api::{
    Channel, ChannelError, ChannelMetadata, ChannelSettingField, PrepareCtx, PreparedRequest,
    RegisteredChannel, SettingControl,
};
use http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderValue};

const BASE_URL: &str = "https://api.openai.com";

pub struct ExampleOpenAiChannel;

impl Channel for ExampleOpenAiChannel {
    fn id(&self) -> &'static str {
        "example-openai"
    }

    fn metadata(&self) -> ChannelMetadata {
        let mut metadata = ChannelMetadata::new(self.id());
        metadata.display_name = "Example OpenAI".into();
        metadata.settings_fields = vec![ChannelSettingField {
            key: "base_url".into(),
            control: SettingControl::Url,
            label: Some("Base URL".into()),
            required: false,
            default: Some(BASE_URL.into()),
            placeholder: Some(BASE_URL.into()),
        }];
        metadata
    }

    fn routing_table(&self) -> RouteList {
        vec![
            routes::pass(ListModels, routes::pv(Provider::OpenAi)),
            routes::pass(GetModel, routes::pv(Provider::OpenAi)),
            routes::pass(GenerateContent, routes::cg(OpenAiResponses)),
            routes::pass(GenerateContent, routes::cg(OpenAiChatCompletions)),
            routes::pass(StreamGenerateContent, routes::cg(OpenAiResponses)),
            routes::pass(StreamGenerateContent, routes::cg(OpenAiChatCompletions)),
        ]
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        let api_key = ctx
            .secret
            .get("api_key")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ChannelError::InvalidCredential("missing api_key".into()))?;
        let base_url = ctx
            .provider_settings
            .get("base_url")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(BASE_URL)
            .trim_end_matches('/');
        let path = if ctx.path.starts_with('/') {
            ctx.path.to_owned()
        } else {
            format!("/{}", ctx.path)
        };
        let mut url = format!("{base_url}{path}");
        if let Some(query) = ctx.query.filter(|query| !query.is_empty()) {
            url.push('?');
            url.push_str(query);
        }
        let uri = url
            .parse::<http::Uri>()
            .map_err(|error| ChannelError::Build(format!("invalid upstream URL: {error}")))?;
        let bearer = HeaderValue::from_str(&format!("Bearer {api_key}")).map_err(|error| {
            ChannelError::InvalidCredential(format!("invalid api_key: {error}"))
        })?;

        // Forward only protocol headers, never downstream credentials or cookies.
        let accept = ctx.headers.get(ACCEPT).cloned();
        let content_type = ctx.headers.get(CONTENT_TYPE).cloned();
        let mut request = http::Request::builder()
            .method(ctx.method)
            .uri(uri)
            .body(ctx.body)
            .map_err(|error| ChannelError::Build(error.to_string()))?;
        request.headers_mut().insert(AUTHORIZATION, bearer);
        if let Some(value) = accept {
            request.headers_mut().insert(ACCEPT, value);
        }
        if let Some(value) = content_type {
            request.headers_mut().insert(CONTENT_TYPE, value);
        }

        Ok(PreparedRequest::new(request))
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn register() -> RegisteredChannel {
    RegisteredChannel::new(Arc::new(ExampleOpenAiChannel))
}

#[cfg(not(target_arch = "wasm32"))]
#[linkme::distributed_slice(gproxy_channel_api::registration::CHANNEL_REGISTRATIONS)]
static REGISTER: gproxy_channel_api::ChannelRegistration = register;

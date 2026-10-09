use std::future::Future;

use super::super::super::{
    Endpoint, GenerationIdentity, GenerationResources, GenerationStateAccess,
    gemini_responses::{GeminiViaResponses, ResponsesViaGemini},
};
use super::super::{ResponsesHistoryCache, StreamInvocation, StreamSettings, StreamTarget};
use super::{RequestMode, start};
use crate::{
    capability::{ResourceAccess, StateStore},
    transform::{
        TransformError,
        generate::{gemini_responses as pair, gemini_responses::stream as p},
    },
    wire::{DeclaredFields, gemini as g, openai::responses as r},
};

pub struct ResponsesViaGeminiStreamFacts {
    pub request: pair::GeminiReplayContext,
    pub response: p::GeminiToResponsesContext,
}

impl GeminiViaResponses {
    pub async fn prepare_stream<S: StateStore>(
        input: g::GenerateContentRequestBody,
        target: StreamTarget,
        context: p::ResponsesToGeminiContext,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<StreamInvocation<p::ResponsesToGeminiStream>, TransformError> {
        Self::stream(
            input,
            target,
            context,
            settings,
            state,
            |input, endpoint, ids| Self::prepare_with_state(input, endpoint, ids, state),
        )
        .await
    }
    pub async fn prepare_stream_with_capabilities<S: StateStore, R: ResourceAccess>(
        input: g::GenerateContentRequestBody,
        target: StreamTarget,
        context: p::ResponsesToGeminiContext,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
    ) -> Result<StreamInvocation<p::ResponsesToGeminiStream>, TransformError> {
        Self::stream(
            input,
            target,
            context,
            settings,
            state,
            |input, endpoint, ids| {
                Self::prepare_with_capabilities(input, endpoint, ids, state, resources)
            },
        )
        .await
    }
    async fn stream<S: StateStore, Fut: Future<Output = Result<Self, TransformError>>>(
        input: g::GenerateContentRequestBody,
        target: StreamTarget,
        mut context: p::ResponsesToGeminiContext,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        prepare: impl FnOnce(g::GenerateContentRequestBody, Endpoint, GenerationIdentity) -> Fut,
    ) -> Result<StreamInvocation<p::ResponsesToGeminiStream>, TransformError> {
        let original = input.into_declared();
        context.response_modalities = original
            .generation_config
            .as_ref()
            .and_then(|v| v.response_modalities.clone());
        context.image_mime = original
            .generation_config
            .as_ref()
            .and_then(|v| v.response_format.as_ref())
            .and_then(|v| v.image.as_ref())
            .and_then(|v| v.mime_type.clone());
        let needs_images = crate::adapt::generate::image_resources::wants_uri(&original);
        let prepared = prepare(
            original.clone().buffered(),
            target.endpoint.clone(),
            target.identities.clone(),
        )
        .await?;
        let mut invocation = start(
            original,
            prepared,
            target,
            settings,
            state,
            |_, target, settings| {
                p::ResponsesToGeminiStream::new_with_policy(
                    context,
                    target.identities.response.clone(),
                    target.identities.response_policy.clone(),
                    settings.events.into(),
                )
            },
        )
        .await?;
        invocation.image_resources_required = needs_images;
        Ok(invocation)
    }
}

impl ResponsesViaGemini {
    pub async fn prepare_stream<S: StateStore>(
        input: r::GenerateContentRequestBody,
        target: StreamTarget,
        context: ResponsesViaGeminiStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<StreamInvocation<p::GeminiToResponsesStream>, TransformError> {
        Self::stream(
            input,
            target,
            context,
            settings,
            state,
            None,
            |input, endpoint, ids, request| {
                Self::prepare_with_state(input, endpoint, ids, state, request)
            },
        )
        .await
    }
    pub async fn prepare_stream_with_history_cache<S: StateStore>(
        input: r::GenerateContentRequestBody,
        target: StreamTarget,
        context: ResponsesViaGeminiStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        history_cache: &ResponsesHistoryCache,
    ) -> Result<StreamInvocation<p::GeminiToResponsesStream>, TransformError> {
        let cache = Some(history_cache);
        Self::stream(
            input,
            target,
            context,
            settings,
            state,
            cache,
            |input, endpoint, ids, request| {
                Self::prepare_with_state(input, endpoint, ids, state, request)
            },
        )
        .await
    }
    pub async fn prepare_stream_with_capabilities<S: StateStore, R: ResourceAccess>(
        input: r::GenerateContentRequestBody,
        target: StreamTarget,
        context: ResponsesViaGeminiStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
    ) -> Result<StreamInvocation<p::GeminiToResponsesStream>, TransformError> {
        Self::stream(
            input,
            target,
            context,
            settings,
            state,
            None,
            |input, endpoint, ids, request| {
                Self::prepare_with_capabilities(input, endpoint, ids, state, resources, request)
            },
        )
        .await
    }
    pub async fn prepare_stream_with_capabilities_and_history_cache<
        S: StateStore,
        R: ResourceAccess,
    >(
        input: r::GenerateContentRequestBody,
        target: StreamTarget,
        context: ResponsesViaGeminiStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
        history_cache: &ResponsesHistoryCache,
    ) -> Result<StreamInvocation<p::GeminiToResponsesStream>, TransformError> {
        let cache = Some(history_cache);
        Self::stream(
            input,
            target,
            context,
            settings,
            state,
            cache,
            |input, endpoint, ids, request| {
                Self::prepare_with_capabilities(input, endpoint, ids, state, resources, request)
            },
        )
        .await
    }
    async fn stream<S: StateStore, Fut: Future<Output = Result<Self, TransformError>>>(
        input: r::GenerateContentRequestBody,
        target: StreamTarget,
        mut context: ResponsesViaGeminiStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        history_cache: Option<&ResponsesHistoryCache>,
        prepare: impl FnOnce(
            r::GenerateContentRequestBody,
            Endpoint,
            GenerationIdentity,
            pair::GeminiReplayContext,
        ) -> Fut,
    ) -> Result<StreamInvocation<p::GeminiToResponsesStream>, TransformError> {
        let original = input.into_declared();
        let (history, expanded) = super::super::history::History::prepare_with_cache(
            &original,
            state,
            settings.codec,
            history_cache,
        )
        .await?;
        context.response.response.request = original.clone();
        context.response.response.request.input = expanded.input.clone();
        if context.response.actual_model.is_none() {
            context.response.actual_model = Some(state.target.model.clone());
        }
        let prepared = prepare(
            expanded.buffered(),
            target.endpoint.clone(),
            target.identities.clone(),
            context.request,
        )
        .await?;
        let mut invocation = start(
            original,
            prepared,
            target,
            settings,
            state,
            |_, target, settings| {
                history.claim_response_id(&mut target.identities, crate::Dialect::Gemini)?;
                p::GeminiToResponsesStream::new_with_policy(
                    context.response,
                    target.identities.response.clone(),
                    target.identities.response_policy.clone(),
                    settings.events.into(),
                )
            },
        )
        .await?;
        history.bind(&invocation.binding, state)?;
        invocation.history = Some(history);
        Ok(invocation)
    }
}

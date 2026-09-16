use super::super::super::{
    GenerationResources, GenerationStateAccess,
    gemini_responses::{GeminiViaResponses, ResponsesViaGemini},
};
use super::super::{StreamInvocation, StreamSettings, StreamTarget};
use super::RequestMode;
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
        mut target: StreamTarget,
        mut context: p::ResponsesToGeminiContext,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
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
        let prepared = Self::prepare_with_state(
            original.clone().buffered(),
            target.endpoint.clone(),
            target.identities.clone(),
            state,
        )
        .await?;
        target.identities = prepared.identities().clone();
        let bridge = p::ResponsesToGeminiStream::new_with_policy(
            context,
            target.identities.response.clone(),
            target.identities.response_policy.clone(),
            settings.events.into(),
        )?;
        let needs_images = crate::adapt::generate::image_resources::wants_uri(&original);
        let mut invocation = StreamInvocation::new(
            original,
            prepared.target_request().clone().streaming(),
            target,
            settings,
            bridge,
            prepared.report().clone(),
            state,
        )
        .await?;
        invocation.image_resources_required = needs_images;
        Ok(invocation)
    }
    pub async fn prepare_stream_with_capabilities<S: StateStore, R: ResourceAccess>(
        input: g::GenerateContentRequestBody,
        mut target: StreamTarget,
        mut context: p::ResponsesToGeminiContext,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
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
        let prepared = Self::prepare_with_capabilities(
            original.clone().buffered(),
            target.endpoint.clone(),
            target.identities.clone(),
            state,
            resources,
        )
        .await?;
        target.identities = prepared.identities().clone();
        let bridge = p::ResponsesToGeminiStream::new_with_policy(
            context,
            target.identities.response.clone(),
            target.identities.response_policy.clone(),
            settings.events.into(),
        )?;
        let needs_images = crate::adapt::generate::image_resources::wants_uri(&original);
        let mut invocation = StreamInvocation::new(
            original,
            prepared.target_request().clone().streaming(),
            target,
            settings,
            bridge,
            prepared.report().clone(),
            state,
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
        Self::prepare_stream_inner(input, target, context, settings, state, None).await
    }
    pub async fn prepare_stream_with_history_cache<S: StateStore>(
        input: r::GenerateContentRequestBody,
        target: StreamTarget,
        context: ResponsesViaGeminiStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        history_cache: &super::super::ResponsesHistoryCache,
    ) -> Result<StreamInvocation<p::GeminiToResponsesStream>, TransformError> {
        Self::prepare_stream_inner(input, target, context, settings, state, Some(history_cache))
            .await
    }
    async fn prepare_stream_inner<S: StateStore>(
        input: r::GenerateContentRequestBody,
        mut target: StreamTarget,
        mut context: ResponsesViaGeminiStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        history_cache: Option<&super::super::ResponsesHistoryCache>,
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
        let prepared = Self::prepare_with_state(
            expanded.buffered(),
            target.endpoint.clone(),
            target.identities.clone(),
            state,
            context.request,
        )
        .await?;
        target.identities = prepared.identities().clone();
        let bridge = p::GeminiToResponsesStream::new_with_policy(
            context.response,
            target.identities.response.clone(),
            target.identities.response_policy.clone(),
            settings.events.into(),
        )?;
        let mut invocation = StreamInvocation::new(
            original,
            prepared.target_request().clone().streaming(),
            target,
            settings,
            bridge,
            prepared.report().clone(),
            state,
        )
        .await?;
        history.bind(&invocation.preparation, state).await?;
        invocation.history = Some(history);
        Ok(invocation)
    }
    pub async fn prepare_stream_with_capabilities<S: StateStore, R: ResourceAccess>(
        input: r::GenerateContentRequestBody,
        target: StreamTarget,
        context: ResponsesViaGeminiStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
    ) -> Result<StreamInvocation<p::GeminiToResponsesStream>, TransformError> {
        Self::prepare_stream_with_capabilities_inner(
            input, target, context, settings, state, resources, None,
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
        history_cache: &super::super::ResponsesHistoryCache,
    ) -> Result<StreamInvocation<p::GeminiToResponsesStream>, TransformError> {
        Self::prepare_stream_with_capabilities_inner(
            input,
            target,
            context,
            settings,
            state,
            resources,
            Some(history_cache),
        )
        .await
    }
    async fn prepare_stream_with_capabilities_inner<S: StateStore, R: ResourceAccess>(
        input: r::GenerateContentRequestBody,
        mut target: StreamTarget,
        mut context: ResponsesViaGeminiStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
        history_cache: Option<&super::super::ResponsesHistoryCache>,
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
        let prepared = Self::prepare_with_capabilities(
            expanded.buffered(),
            target.endpoint.clone(),
            target.identities.clone(),
            state,
            resources,
            context.request,
        )
        .await?;
        target.identities = prepared.identities().clone();
        let bridge = p::GeminiToResponsesStream::new_with_policy(
            context.response,
            target.identities.response.clone(),
            target.identities.response_policy.clone(),
            settings.events.into(),
        )?;
        let mut invocation = StreamInvocation::new(
            original,
            prepared.target_request().clone().streaming(),
            target,
            settings,
            bridge,
            prepared.report().clone(),
            state,
        )
        .await?;
        history.bind(&invocation.preparation, state).await?;
        invocation.history = Some(history);
        Ok(invocation)
    }
}

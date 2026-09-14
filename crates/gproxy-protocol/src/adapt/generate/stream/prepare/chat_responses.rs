use super::super::super::{
    GenerationResources, GenerationStateAccess,
    chat_responses::{ChatViaResponses, ResponsesViaChat},
};
use super::super::{StreamInvocation, StreamSettings, StreamTarget};
use super::RequestMode;
use crate::{
    capability::{ResourceAccess, StateStore},
    transform::{TransformError, generate::chat_responses::stream as p},
    wire::{
        DeclaredFields,
        openai::{chat as h, responses as r},
    },
};

impl ChatViaResponses {
    pub async fn prepare_stream<S: StateStore>(
        input: h::GenerateContentRequestBody,
        mut target: StreamTarget,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<StreamInvocation<p::ResponsesToChatStream>, TransformError> {
        settings.validate::<p::ResponsesToChatStream>()?;
        let original = input.into_declared();
        let prepared = Self::prepare_with_state(
            original.clone().buffered(),
            target.model.clone(),
            target.endpoint.clone(),
            target.identities.clone(),
            state,
        )
        .await?;
        target.identities = prepared.identities().clone();
        let bridge = p::ResponsesToChatStream::new_with_policy(
            target.identities.response.clone(),
            settings.events.into(),
            target.identities.response_policy.clone(),
        )?;
        let emit_usage = original.emit_usage();
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
        invocation.emit_usage = emit_usage;
        Ok(invocation)
    }
    pub async fn prepare_stream_with_capabilities<S: StateStore, R: ResourceAccess>(
        input: h::GenerateContentRequestBody,
        mut target: StreamTarget,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
    ) -> Result<StreamInvocation<p::ResponsesToChatStream>, TransformError> {
        settings.validate::<p::ResponsesToChatStream>()?;
        let original = input.into_declared();
        let prepared = Self::prepare_with_capabilities(
            original.clone().buffered(),
            target.model.clone(),
            target.endpoint.clone(),
            target.identities.clone(),
            state,
            resources,
        )
        .await?;
        target.identities = prepared.identities().clone();
        let bridge = p::ResponsesToChatStream::new_with_policy(
            target.identities.response.clone(),
            settings.events.into(),
            target.identities.response_policy.clone(),
        )?;
        let emit_usage = original.emit_usage();
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
        invocation.emit_usage = emit_usage;
        Ok(invocation)
    }
}
impl ResponsesViaChat {
    pub async fn prepare_stream<S: StateStore>(
        input: r::GenerateContentRequestBody,
        target: StreamTarget,
        context: p::ChatToResponsesContext,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<StreamInvocation<p::ChatToResponsesStream>, TransformError> {
        Self::prepare_stream_inner(input, target, context, settings, state, None).await
    }
    pub async fn prepare_stream_with_history_cache<S: StateStore>(
        input: r::GenerateContentRequestBody,
        target: StreamTarget,
        context: p::ChatToResponsesContext,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        history_cache: &super::super::ResponsesHistoryCache,
    ) -> Result<StreamInvocation<p::ChatToResponsesStream>, TransformError> {
        Self::prepare_stream_inner(input, target, context, settings, state, Some(history_cache))
            .await
    }
    async fn prepare_stream_inner<S: StateStore>(
        input: r::GenerateContentRequestBody,
        mut target: StreamTarget,
        mut context: p::ChatToResponsesContext,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        history_cache: Option<&super::super::ResponsesHistoryCache>,
    ) -> Result<StreamInvocation<p::ChatToResponsesStream>, TransformError> {
        settings.validate::<p::ChatToResponsesStream>()?;
        let original = input.into_declared();
        let (history, expanded) = super::super::history::History::prepare_with_cache(
            &original,
            state,
            settings.codec,
            history_cache,
        )
        .await?;
        context.response.request = original.clone();
        let prepared = Self::prepare_with_state(
            expanded.buffered(),
            target.model.clone(),
            target.endpoint.clone(),
            target.identities.clone(),
            state,
        )
        .await?;
        target.identities = prepared.identities().clone();
        let bridge = p::ChatToResponsesStream::new_with_policy(
            context,
            target.identities.response.clone(),
            settings.events.into(),
            target.identities.response_policy.clone(),
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
        context: p::ChatToResponsesContext,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
    ) -> Result<StreamInvocation<p::ChatToResponsesStream>, TransformError> {
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
        context: p::ChatToResponsesContext,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
        history_cache: &super::super::ResponsesHistoryCache,
    ) -> Result<StreamInvocation<p::ChatToResponsesStream>, TransformError> {
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
        mut context: p::ChatToResponsesContext,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
        history_cache: Option<&super::super::ResponsesHistoryCache>,
    ) -> Result<StreamInvocation<p::ChatToResponsesStream>, TransformError> {
        settings.validate::<p::ChatToResponsesStream>()?;
        let original = input.into_declared();
        let (history, expanded) = super::super::history::History::prepare_with_cache(
            &original,
            state,
            settings.codec,
            history_cache,
        )
        .await?;
        context.response.request = original.clone();
        let prepared = Self::prepare_with_capabilities(
            expanded.buffered(),
            target.model.clone(),
            target.endpoint.clone(),
            target.identities.clone(),
            state,
            resources,
        )
        .await?;
        target.identities = prepared.identities().clone();
        let bridge = p::ChatToResponsesStream::new_with_policy(
            context,
            target.identities.response.clone(),
            settings.events.into(),
            target.identities.response_policy.clone(),
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

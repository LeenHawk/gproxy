use super::super::super::{
    GenerationResources, GenerationStateAccess,
    claude_responses::{ClaudeViaResponses, ResponsesViaClaude},
};
use super::super::{StreamInvocation, StreamSettings, StreamTarget};
use super::RequestMode;
use crate::{
    capability::{ResourceAccess, StateStore},
    transform::{
        TransformError,
        generate::{claude_responses as pair, claude_responses::stream as p},
    },
    wire::{DeclaredFields, claude::generate_content as c, openai::responses as r},
};

pub struct ResponsesViaClaudeStreamFacts {
    pub request: pair::ClaudeRequestContext,
    pub response: p::ClaudeToResponsesContext,
}

impl ClaudeViaResponses {
    pub async fn prepare_stream<S: StateStore>(
        input: c::GenerateContentRequestBody,
        mut target: StreamTarget,
        context: p::ResponsesToClaudeContext,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<StreamInvocation<p::ResponsesToClaudeStream>, TransformError> {
        let original = input.into_declared();
        let prepared = Self::prepare_with_state(
            original.clone().buffered(),
            target.endpoint.clone(),
            target.identities.clone(),
            state,
        )
        .await?;
        target.identities = prepared.identities().clone();
        let bridge = p::ResponsesToClaudeStream::new_with_policy(
            context,
            target.identities.response.clone(),
            settings.events.into(),
            target.identities.response_policy.clone(),
        )?;
        StreamInvocation::new(
            original,
            prepared.target_request().clone().streaming(),
            target,
            settings,
            bridge,
            prepared.report().clone(),
            state,
        )
        .await
    }
    pub async fn prepare_stream_with_capabilities<S: StateStore, R: ResourceAccess>(
        input: c::GenerateContentRequestBody,
        mut target: StreamTarget,
        context: p::ResponsesToClaudeContext,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
    ) -> Result<StreamInvocation<p::ResponsesToClaudeStream>, TransformError> {
        let original = input.into_declared();
        let prepared = Self::prepare_with_capabilities(
            original.clone().buffered(),
            target.endpoint.clone(),
            target.identities.clone(),
            state,
            resources,
        )
        .await?;
        target.identities = prepared.identities().clone();
        let bridge = p::ResponsesToClaudeStream::new_with_policy(
            context,
            target.identities.response.clone(),
            settings.events.into(),
            target.identities.response_policy.clone(),
        )?;
        StreamInvocation::new(
            original,
            prepared.target_request().clone().streaming(),
            target,
            settings,
            bridge,
            prepared.report().clone(),
            state,
        )
        .await
    }
}

impl ResponsesViaClaude {
    pub async fn prepare_stream<S: StateStore>(
        input: r::GenerateContentRequestBody,
        target: StreamTarget,
        context: ResponsesViaClaudeStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<StreamInvocation<p::ClaudeToResponsesStream>, TransformError> {
        Self::prepare_stream_inner(input, target, context, settings, state, None).await
    }
    pub async fn prepare_stream_with_history_cache<S: StateStore>(
        input: r::GenerateContentRequestBody,
        target: StreamTarget,
        context: ResponsesViaClaudeStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        history_cache: &super::super::ResponsesHistoryCache,
    ) -> Result<StreamInvocation<p::ClaudeToResponsesStream>, TransformError> {
        Self::prepare_stream_inner(input, target, context, settings, state, Some(history_cache))
            .await
    }
    async fn prepare_stream_inner<S: StateStore>(
        input: r::GenerateContentRequestBody,
        mut target: StreamTarget,
        mut context: ResponsesViaClaudeStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        history_cache: Option<&super::super::ResponsesHistoryCache>,
    ) -> Result<StreamInvocation<p::ClaudeToResponsesStream>, TransformError> {
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
        let prepared = Self::prepare_with_state(
            expanded.buffered(),
            target.endpoint.clone(),
            target.identities.clone(),
            state,
            context.request,
        )
        .await?;
        target.identities = prepared.identities().clone();
        let bridge = p::ClaudeToResponsesStream::new_with_policy(
            context.response,
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
        context: ResponsesViaClaudeStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
    ) -> Result<StreamInvocation<p::ClaudeToResponsesStream>, TransformError> {
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
        context: ResponsesViaClaudeStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
        history_cache: &super::super::ResponsesHistoryCache,
    ) -> Result<StreamInvocation<p::ClaudeToResponsesStream>, TransformError> {
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
        mut context: ResponsesViaClaudeStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
        history_cache: Option<&super::super::ResponsesHistoryCache>,
    ) -> Result<StreamInvocation<p::ClaudeToResponsesStream>, TransformError> {
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
        let bridge = p::ClaudeToResponsesStream::new_with_policy(
            context.response,
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

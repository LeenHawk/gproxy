use std::future::Future;

use super::super::super::{
    Endpoint, GenerationIdentity, GenerationResources, GenerationStateAccess,
    claude_responses::{ClaudeViaResponses, ResponsesViaClaude},
};
use super::super::{ResponsesHistoryCache, StreamInvocation, StreamSettings, StreamTarget};
use super::{RequestMode, start};
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
        target: StreamTarget,
        context: p::ResponsesToClaudeContext,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<StreamInvocation<p::ResponsesToClaudeStream>, TransformError> {
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
        input: c::GenerateContentRequestBody,
        target: StreamTarget,
        context: p::ResponsesToClaudeContext,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
    ) -> Result<StreamInvocation<p::ResponsesToClaudeStream>, TransformError> {
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
        input: c::GenerateContentRequestBody,
        target: StreamTarget,
        context: p::ResponsesToClaudeContext,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        prepare: impl FnOnce(c::GenerateContentRequestBody, Endpoint, GenerationIdentity) -> Fut,
    ) -> Result<StreamInvocation<p::ResponsesToClaudeStream>, TransformError> {
        let original = input.into_declared();
        let prepared = prepare(
            original.clone().buffered(),
            target.endpoint.clone(),
            target.identities.clone(),
        )
        .await?;
        start(
            original,
            prepared,
            target,
            settings,
            state,
            |_, target, settings| {
                p::ResponsesToClaudeStream::new_with_policy(
                    context,
                    target.identities.response.clone(),
                    settings.events.into(),
                    target.identities.response_policy.clone(),
                )
            },
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
        context: ResponsesViaClaudeStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        history_cache: &ResponsesHistoryCache,
    ) -> Result<StreamInvocation<p::ClaudeToResponsesStream>, TransformError> {
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
        context: ResponsesViaClaudeStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
    ) -> Result<StreamInvocation<p::ClaudeToResponsesStream>, TransformError> {
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
        context: ResponsesViaClaudeStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
        history_cache: &ResponsesHistoryCache,
    ) -> Result<StreamInvocation<p::ClaudeToResponsesStream>, TransformError> {
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
        mut context: ResponsesViaClaudeStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        history_cache: Option<&ResponsesHistoryCache>,
        prepare: impl FnOnce(
            r::GenerateContentRequestBody,
            Endpoint,
            GenerationIdentity,
            pair::ClaudeRequestContext,
        ) -> Fut,
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
                history.claim_response_id(&mut target.identities, crate::Dialect::Claude)?;
                p::ClaudeToResponsesStream::new_with_policy(
                    context.response,
                    target.identities.response.clone(),
                    settings.events.into(),
                    target.identities.response_policy.clone(),
                )
            },
        )
        .await?;
        history.bind(&invocation.binding, state)?;
        invocation.history = Some(history);
        Ok(invocation)
    }
}

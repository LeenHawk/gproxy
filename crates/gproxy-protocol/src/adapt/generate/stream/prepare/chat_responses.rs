use std::future::Future;

use super::super::super::{
    Endpoint, GenerationIdentity, GenerationResources, GenerationStateAccess,
    chat_responses::{ChatViaResponses, ResponsesViaChat},
};
use super::super::{ResponsesHistoryCache, StreamInvocation, StreamSettings, StreamTarget};
use super::{RequestMode, start};
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
        target: StreamTarget,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<StreamInvocation<p::ResponsesToChatStream>, TransformError> {
        Self::stream(input, target, settings, state, |input, endpoint, ids| {
            Self::prepare_with_state(input, endpoint, ids, state)
        })
        .await
    }
    pub async fn prepare_stream_with_capabilities<S: StateStore, R: ResourceAccess>(
        input: h::GenerateContentRequestBody,
        target: StreamTarget,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
    ) -> Result<StreamInvocation<p::ResponsesToChatStream>, TransformError> {
        Self::stream(input, target, settings, state, |input, endpoint, ids| {
            Self::prepare_with_capabilities(input, endpoint, ids, state, resources)
        })
        .await
    }
    async fn stream<S: StateStore, Fut: Future<Output = Result<Self, TransformError>>>(
        input: h::GenerateContentRequestBody,
        target: StreamTarget,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        prepare: impl FnOnce(h::GenerateContentRequestBody, Endpoint, GenerationIdentity) -> Fut,
    ) -> Result<StreamInvocation<p::ResponsesToChatStream>, TransformError> {
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
                p::ResponsesToChatStream::new_with_policy(
                    target.identities.response.clone(),
                    settings.events.into(),
                    target.identities.response_policy.clone(),
                )
            },
        )
        .await
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
        Self::stream(
            input,
            target,
            context,
            settings,
            state,
            None,
            |input, endpoint, ids| Self::prepare_with_state(input, endpoint, ids, state),
        )
        .await
    }
    pub async fn prepare_stream_with_history_cache<S: StateStore>(
        input: r::GenerateContentRequestBody,
        target: StreamTarget,
        context: p::ChatToResponsesContext,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        history_cache: &ResponsesHistoryCache,
    ) -> Result<StreamInvocation<p::ChatToResponsesStream>, TransformError> {
        let cache = Some(history_cache);
        Self::stream(
            input,
            target,
            context,
            settings,
            state,
            cache,
            |input, endpoint, ids| Self::prepare_with_state(input, endpoint, ids, state),
        )
        .await
    }
    pub async fn prepare_stream_with_capabilities<S: StateStore, R: ResourceAccess>(
        input: r::GenerateContentRequestBody,
        target: StreamTarget,
        context: p::ChatToResponsesContext,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
    ) -> Result<StreamInvocation<p::ChatToResponsesStream>, TransformError> {
        Self::stream(
            input,
            target,
            context,
            settings,
            state,
            None,
            |input, endpoint, ids| {
                Self::prepare_with_capabilities(input, endpoint, ids, state, resources)
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
        context: p::ChatToResponsesContext,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
        history_cache: &ResponsesHistoryCache,
    ) -> Result<StreamInvocation<p::ChatToResponsesStream>, TransformError> {
        let cache = Some(history_cache);
        Self::stream(
            input,
            target,
            context,
            settings,
            state,
            cache,
            |input, endpoint, ids| {
                Self::prepare_with_capabilities(input, endpoint, ids, state, resources)
            },
        )
        .await
    }
    async fn stream<S: StateStore, Fut: Future<Output = Result<Self, TransformError>>>(
        input: r::GenerateContentRequestBody,
        target: StreamTarget,
        mut context: p::ChatToResponsesContext,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        history_cache: Option<&ResponsesHistoryCache>,
        prepare: impl FnOnce(r::GenerateContentRequestBody, Endpoint, GenerationIdentity) -> Fut,
    ) -> Result<StreamInvocation<p::ChatToResponsesStream>, TransformError> {
        let original = input.into_declared();
        let (history, expanded) = super::super::history::History::prepare_with_cache(
            &original,
            state,
            settings.codec,
            history_cache,
        )
        .await?;
        context.response.request = original.clone();
        context.response.request.input = expanded.input.clone();
        let prepared = prepare(
            expanded.buffered(),
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
                history.claim_response_id(&mut target.identities, crate::Dialect::OpenAiChat)?;
                p::ChatToResponsesStream::new_with_policy(
                    context,
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

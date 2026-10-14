use std::future::Future;

use super::super::super::{
    Endpoint, GenerationIdentity, GenerationResources, GenerationStateAccess,
    chat_claude::{ChatViaClaude, ClaudeViaChat},
};
use super::super::{StreamInvocation, StreamSettings, StreamTarget};
use super::{RequestMode, start};
use crate::{
    capability::{ResourceAccess, StateStore},
    transform::{TransformError, generate::claude_chat::stream as p},
    wire::{DeclaredFields, claude::generate_content as c, openai::chat as h},
};

impl ChatViaClaude {
    pub async fn prepare_stream<S: StateStore>(
        input: h::GenerateContentRequestBody,
        target: StreamTarget,
        context: p::ClaudeToChatContext,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<StreamInvocation<p::ClaudeToChatStream>, TransformError> {
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
        input: h::GenerateContentRequestBody,
        target: StreamTarget,
        context: p::ClaudeToChatContext,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
    ) -> Result<StreamInvocation<p::ClaudeToChatStream>, TransformError> {
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
        input: h::GenerateContentRequestBody,
        target: StreamTarget,
        context: p::ClaudeToChatContext,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        prepare: impl FnOnce(h::GenerateContentRequestBody, Endpoint, GenerationIdentity) -> Fut,
    ) -> Result<StreamInvocation<p::ClaudeToChatStream>, TransformError> {
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
                p::ClaudeToChatStream::new_with_policy(
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

impl ClaudeViaChat {
    pub async fn prepare_stream<S: StateStore>(
        input: c::GenerateContentRequestBody,
        target: StreamTarget,
        context: p::ChatToClaudeContext,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<StreamInvocation<p::ChatToClaudeStream>, TransformError> {
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
        context: p::ChatToClaudeContext,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
    ) -> Result<StreamInvocation<p::ChatToClaudeStream>, TransformError> {
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
        context: p::ChatToClaudeContext,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        prepare: impl FnOnce(c::GenerateContentRequestBody, Endpoint, GenerationIdentity) -> Fut,
    ) -> Result<StreamInvocation<p::ChatToClaudeStream>, TransformError> {
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
                p::ChatToClaudeStream::new_with_policy(
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

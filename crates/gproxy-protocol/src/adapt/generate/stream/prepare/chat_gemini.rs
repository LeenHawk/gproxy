use std::future::Future;

use super::super::super::{
    Endpoint, GenerationIdentity, GenerationResources, GenerationStateAccess,
    chat_gemini::{ChatViaGemini, GeminiViaChat},
};
use super::super::{StreamInvocation, StreamSettings, StreamTarget};
use super::{RequestMode, start};
use crate::{
    capability::{ResourceAccess, StateStore},
    transform::{TransformError, generate::gemini_chat::stream as p},
    wire::{DeclaredFields, gemini as g, openai::chat as h},
};

pub struct ChatViaGeminiStreamFacts {
    pub function_names: std::collections::BTreeMap<String, String>,
    pub response: p::GeminiToChatContext,
}

impl ChatViaGemini {
    pub async fn prepare_stream<S: StateStore>(
        input: h::GenerateContentRequestBody,
        target: StreamTarget,
        context: ChatViaGeminiStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<StreamInvocation<p::GeminiToChatStream>, TransformError> {
        let ChatViaGeminiStreamFacts {
            function_names,
            response,
        } = context;
        Self::stream(
            input,
            target,
            response,
            settings,
            state,
            |input, endpoint, ids| {
                Self::prepare_with_state(input, endpoint, ids, state, &function_names)
            },
        )
        .await
    }
    pub async fn prepare_stream_with_capabilities<S: StateStore, R: ResourceAccess>(
        input: h::GenerateContentRequestBody,
        target: StreamTarget,
        context: ChatViaGeminiStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
    ) -> Result<StreamInvocation<p::GeminiToChatStream>, TransformError> {
        let ChatViaGeminiStreamFacts {
            function_names,
            response,
        } = context;
        Self::stream(
            input,
            target,
            response,
            settings,
            state,
            |input, endpoint, ids| {
                Self::prepare_with_capabilities(
                    input,
                    endpoint,
                    ids,
                    state,
                    resources,
                    &function_names,
                )
            },
        )
        .await
    }
    async fn stream<S: StateStore, Fut: Future<Output = Result<Self, TransformError>>>(
        input: h::GenerateContentRequestBody,
        target: StreamTarget,
        response: p::GeminiToChatContext,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        prepare: impl FnOnce(h::GenerateContentRequestBody, Endpoint, GenerationIdentity) -> Fut,
    ) -> Result<StreamInvocation<p::GeminiToChatStream>, TransformError> {
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
                p::GeminiToChatStream::new_with_policy(
                    response,
                    target.identities.response.clone(),
                    settings.events.into(),
                    target.identities.response_policy.clone(),
                )
            },
        )
        .await
    }
}

impl GeminiViaChat {
    pub async fn prepare_stream<S: StateStore>(
        input: g::GenerateContentRequestBody,
        target: StreamTarget,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<StreamInvocation<p::ChatToGeminiStream>, TransformError> {
        Self::stream(input, target, settings, state, |input, endpoint, ids| {
            Self::prepare_with_state(input, endpoint, ids, state)
        })
        .await
    }
    pub async fn prepare_stream_with_capabilities<S: StateStore, R: ResourceAccess>(
        input: g::GenerateContentRequestBody,
        target: StreamTarget,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
    ) -> Result<StreamInvocation<p::ChatToGeminiStream>, TransformError> {
        Self::stream(input, target, settings, state, |input, endpoint, ids| {
            Self::prepare_with_capabilities(input, endpoint, ids, state, resources)
        })
        .await
    }
    async fn stream<S: StateStore, Fut: Future<Output = Result<Self, TransformError>>>(
        input: g::GenerateContentRequestBody,
        target: StreamTarget,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        prepare: impl FnOnce(g::GenerateContentRequestBody, Endpoint, GenerationIdentity) -> Fut,
    ) -> Result<StreamInvocation<p::ChatToGeminiStream>, TransformError> {
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
                p::ChatToGeminiStream::new_with_policy(
                    target.identities.response.clone(),
                    settings.events.into(),
                    target.identities.response_policy.clone(),
                )
            },
        )
        .await
    }
}

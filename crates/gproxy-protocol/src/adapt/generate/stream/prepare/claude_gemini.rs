use std::future::Future;

use super::super::super::{
    Endpoint, GenerationIdentity, GenerationResources, GenerationStateAccess,
    claude_gemini::{ClaudeViaGemini, GeminiViaClaude},
};
use super::super::{StreamInvocation, StreamSettings, StreamTarget};
use super::{RequestMode, start};
use crate::{
    capability::{ResourceAccess, StateStore},
    transform::{
        TransformError,
        generate::{claude_gemini as pair, claude_gemini::stream as p},
    },
    wire::{DeclaredFields, claude::generate_content as c, gemini as g},
};

pub struct ClaudeViaGeminiStreamFacts {
    pub request: pair::ClaudeGeminiRequestContext,
    pub response: p::GeminiToClaudeContext,
}

pub struct GeminiViaClaudeStreamFacts {
    pub max_tokens: Option<i64>,
    pub response: p::ClaudeToGeminiContext,
}

impl ClaudeViaGemini {
    pub async fn prepare_stream<S: StateStore>(
        input: c::GenerateContentRequestBody,
        target: StreamTarget,
        context: ClaudeViaGeminiStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<StreamInvocation<p::GeminiToClaudeStream>, TransformError> {
        Self::stream(
            input,
            target,
            context,
            settings,
            state,
            |input, endpoint, ids, request| {
                Self::prepare_with_state(input, endpoint, ids, state, request)
            },
        )
        .await
    }
    pub async fn prepare_stream_with_capabilities<S: StateStore, R: ResourceAccess>(
        input: c::GenerateContentRequestBody,
        target: StreamTarget,
        context: ClaudeViaGeminiStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
    ) -> Result<StreamInvocation<p::GeminiToClaudeStream>, TransformError> {
        Self::stream(
            input,
            target,
            context,
            settings,
            state,
            |input, endpoint, ids, request| {
                Self::prepare_with_capabilities(input, endpoint, ids, state, resources, request)
            },
        )
        .await
    }
    async fn stream<S: StateStore, Fut: Future<Output = Result<Self, TransformError>>>(
        input: c::GenerateContentRequestBody,
        target: StreamTarget,
        context: ClaudeViaGeminiStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        prepare: impl FnOnce(
            c::GenerateContentRequestBody,
            Endpoint,
            GenerationIdentity,
            pair::ClaudeGeminiRequestContext,
        ) -> Fut,
    ) -> Result<StreamInvocation<p::GeminiToClaudeStream>, TransformError> {
        let original = input.into_declared();
        let prepared = prepare(
            original.clone().buffered(),
            target.endpoint.clone(),
            target.identities.clone(),
            context.request,
        )
        .await?;
        start(
            original,
            prepared,
            target,
            settings,
            state,
            |_, target, settings| {
                p::GeminiToClaudeStream::new_with_policy(
                    context.response,
                    target.identities.response.clone(),
                    settings.events.into(),
                    target.identities.response_policy.clone(),
                )
            },
        )
        .await
    }
}

impl GeminiViaClaude {
    pub async fn prepare_stream<S: StateStore>(
        input: g::GenerateContentRequestBody,
        target: StreamTarget,
        context: GeminiViaClaudeStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<StreamInvocation<p::ClaudeToGeminiStream>, TransformError> {
        Self::stream(
            input,
            target,
            context,
            settings,
            state,
            |input, endpoint, ids, max| Self::prepare_with_state(input, endpoint, ids, state, max),
        )
        .await
    }
    pub async fn prepare_stream_with_capabilities<S: StateStore, R: ResourceAccess>(
        input: g::GenerateContentRequestBody,
        target: StreamTarget,
        context: GeminiViaClaudeStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
    ) -> Result<StreamInvocation<p::ClaudeToGeminiStream>, TransformError> {
        Self::stream(
            input,
            target,
            context,
            settings,
            state,
            |input, endpoint, ids, max| {
                Self::prepare_with_capabilities(input, endpoint, ids, state, resources, max)
            },
        )
        .await
    }
    async fn stream<S: StateStore, Fut: Future<Output = Result<Self, TransformError>>>(
        input: g::GenerateContentRequestBody,
        target: StreamTarget,
        mut context: GeminiViaClaudeStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        prepare: impl FnOnce(
            g::GenerateContentRequestBody,
            Endpoint,
            GenerationIdentity,
            Option<i64>,
        ) -> Fut,
    ) -> Result<StreamInvocation<p::ClaudeToGeminiStream>, TransformError> {
        let original = input.into_declared();
        let prepared = prepare(
            original.clone().buffered(),
            target.endpoint.clone(),
            target.identities.clone(),
            context.max_tokens,
        )
        .await?;
        start(
            original,
            prepared,
            target,
            settings,
            state,
            |prepared, target, settings| {
                context.response.usage = prepared.usage_facts(context.response.usage);
                p::ClaudeToGeminiStream::new_with_policy(
                    context.response,
                    target.identities.response.clone(),
                    settings.events.into(),
                    target.identities.response_policy.clone(),
                )
            },
        )
        .await
    }
}

use super::super::super::{
    GenerationResources, GenerationStateAccess,
    claude_gemini::{ClaudeViaGemini, GeminiViaClaude},
};
use super::super::{StreamInvocation, StreamSettings, StreamTarget};
use super::RequestMode;
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
        mut target: StreamTarget,
        context: ClaudeViaGeminiStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<StreamInvocation<p::GeminiToClaudeStream>, TransformError> {
        settings.validate::<p::GeminiToClaudeStream>()?;
        let original = input.into_declared();
        let prepared = Self::prepare_with_state(
            original.clone().buffered(),
            target.model.clone(),
            target.endpoint.clone(),
            target.identities.clone(),
            state,
            context.request,
        )
        .await?;
        target.identities = prepared.identities().clone();
        let bridge = p::GeminiToClaudeStream::new_with_policy(
            context.response,
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
        context: ClaudeViaGeminiStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
    ) -> Result<StreamInvocation<p::GeminiToClaudeStream>, TransformError> {
        settings.validate::<p::GeminiToClaudeStream>()?;
        let original = input.into_declared();
        let prepared = Self::prepare_with_capabilities(
            original.clone().buffered(),
            target.model.clone(),
            target.endpoint.clone(),
            target.identities.clone(),
            state,
            resources,
            context.request,
        )
        .await?;
        target.identities = prepared.identities().clone();
        let bridge = p::GeminiToClaudeStream::new_with_policy(
            context.response,
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
impl GeminiViaClaude {
    pub async fn prepare_stream<S: StateStore>(
        input: g::GenerateContentRequestBody,
        mut target: StreamTarget,
        context: GeminiViaClaudeStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<StreamInvocation<p::ClaudeToGeminiStream>, TransformError> {
        settings.validate::<p::ClaudeToGeminiStream>()?;
        let original = input.into_declared();
        let prepared = Self::prepare_with_state(
            original.clone().buffered(),
            target.model.clone(),
            target.endpoint.clone(),
            target.identities.clone(),
            state,
            context.max_tokens,
        )
        .await?;
        target.identities = prepared.identities().clone();
        let bridge = p::ClaudeToGeminiStream::new_with_policy(
            context.response,
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
        input: g::GenerateContentRequestBody,
        mut target: StreamTarget,
        context: GeminiViaClaudeStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
    ) -> Result<StreamInvocation<p::ClaudeToGeminiStream>, TransformError> {
        settings.validate::<p::ClaudeToGeminiStream>()?;
        let original = input.into_declared();
        let prepared = Self::prepare_with_capabilities(
            original.clone().buffered(),
            target.model.clone(),
            target.endpoint.clone(),
            target.identities.clone(),
            state,
            resources,
            context.max_tokens,
        )
        .await?;
        target.identities = prepared.identities().clone();
        let bridge = p::ClaudeToGeminiStream::new_with_policy(
            context.response,
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

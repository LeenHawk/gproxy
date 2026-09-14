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
        settings.validate::<p::ResponsesToClaudeStream>()?;
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
        settings.validate::<p::ResponsesToClaudeStream>()?;
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
        mut target: StreamTarget,
        mut context: ResponsesViaClaudeStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<StreamInvocation<p::ClaudeToResponsesStream>, TransformError> {
        settings.validate::<p::ClaudeToResponsesStream>()?;
        let original = input.into_declared();
        context.response.response.request = original.clone();
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
        let bridge = p::ClaudeToResponsesStream::new_with_policy(
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
        input: r::GenerateContentRequestBody,
        mut target: StreamTarget,
        mut context: ResponsesViaClaudeStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
    ) -> Result<StreamInvocation<p::ClaudeToResponsesStream>, TransformError> {
        settings.validate::<p::ClaudeToResponsesStream>()?;
        let original = input.into_declared();
        context.response.response.request = original.clone();
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
        let bridge = p::ClaudeToResponsesStream::new_with_policy(
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

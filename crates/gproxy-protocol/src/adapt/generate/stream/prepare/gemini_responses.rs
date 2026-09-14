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
        context: p::ResponsesToGeminiContext,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<StreamInvocation<p::ResponsesToGeminiStream>, TransformError> {
        settings.validate::<p::ResponsesToGeminiStream>()?;
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
        let bridge = p::ResponsesToGeminiStream::new_with_policy(
            context,
            target.identities.response.clone(),
            target.identities.response_policy.clone(),
            settings.events.into(),
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
        context: p::ResponsesToGeminiContext,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
    ) -> Result<StreamInvocation<p::ResponsesToGeminiStream>, TransformError> {
        settings.validate::<p::ResponsesToGeminiStream>()?;
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
        let bridge = p::ResponsesToGeminiStream::new_with_policy(
            context,
            target.identities.response.clone(),
            target.identities.response_policy.clone(),
            settings.events.into(),
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
impl ResponsesViaGemini {
    pub async fn prepare_stream<S: StateStore>(
        input: r::GenerateContentRequestBody,
        mut target: StreamTarget,
        mut context: ResponsesViaGeminiStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<StreamInvocation<p::GeminiToResponsesStream>, TransformError> {
        settings.validate::<p::GeminiToResponsesStream>()?;
        let original = input.into_declared();
        context.response.response.request = original.clone();
        if context.response.actual_model.is_none() {
            context.response.actual_model = Some(target.model.clone());
        }
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
        let bridge = p::GeminiToResponsesStream::new_with_policy(
            context.response,
            target.identities.response.clone(),
            target.identities.response_policy.clone(),
            settings.events.into(),
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
        mut context: ResponsesViaGeminiStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
    ) -> Result<StreamInvocation<p::GeminiToResponsesStream>, TransformError> {
        settings.validate::<p::GeminiToResponsesStream>()?;
        let original = input.into_declared();
        context.response.response.request = original.clone();
        if context.response.actual_model.is_none() {
            context.response.actual_model = Some(target.model.clone());
        }
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
        let bridge = p::GeminiToResponsesStream::new_with_policy(
            context.response,
            target.identities.response.clone(),
            target.identities.response_policy.clone(),
            settings.events.into(),
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

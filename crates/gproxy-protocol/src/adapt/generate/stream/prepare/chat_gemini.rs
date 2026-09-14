use super::super::super::{
    GenerationResources, GenerationStateAccess,
    chat_gemini::{ChatViaGemini, GeminiViaChat},
};
use super::super::{StreamInvocation, StreamSettings, StreamTarget};
use super::RequestMode;
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
        mut target: StreamTarget,
        context: ChatViaGeminiStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<StreamInvocation<p::GeminiToChatStream>, TransformError> {
        settings.validate::<p::GeminiToChatStream>()?;
        let original = input.into_declared();
        let prepared = Self::prepare_with_state(
            original.clone().buffered(),
            target.model.clone(),
            target.endpoint.clone(),
            target.identities.clone(),
            state,
            &context.function_names,
        )
        .await?;
        target.identities = prepared.identities().clone();
        let bridge = p::GeminiToChatStream::new_with_policy(
            context.response,
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
        context: ChatViaGeminiStreamFacts,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
    ) -> Result<StreamInvocation<p::GeminiToChatStream>, TransformError> {
        settings.validate::<p::GeminiToChatStream>()?;
        let original = input.into_declared();
        let prepared = Self::prepare_with_capabilities(
            original.clone().buffered(),
            target.model.clone(),
            target.endpoint.clone(),
            target.identities.clone(),
            state,
            resources,
            &context.function_names,
        )
        .await?;
        target.identities = prepared.identities().clone();
        let bridge = p::GeminiToChatStream::new_with_policy(
            context.response,
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
impl GeminiViaChat {
    pub async fn prepare_stream<S: StateStore>(
        input: g::GenerateContentRequestBody,
        mut target: StreamTarget,
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<StreamInvocation<p::ChatToGeminiStream>, TransformError> {
        settings.validate::<p::ChatToGeminiStream>()?;
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
        let bridge = p::ChatToGeminiStream::new_with_policy(
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
        settings: StreamSettings,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
    ) -> Result<StreamInvocation<p::ChatToGeminiStream>, TransformError> {
        settings.validate::<p::ChatToGeminiStream>()?;
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
        let bridge = p::ChatToGeminiStream::new_with_policy(
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

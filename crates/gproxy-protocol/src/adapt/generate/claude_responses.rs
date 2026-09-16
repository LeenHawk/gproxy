use super::{Endpoint, GenerationIdentity, GenerationOutcome, GenerationProgress, transport};
use crate::transform::generate::claude_responses as p;
use crate::wire::claude::generate_content as c;
use crate::wire::openai::responses as r;
use crate::{
    Dialect,
    capability::Upstream,
    codec::CodecLimits,
    transform::{Converted, Report, TransformError},
    wire::DeclaredFields,
};
/// A prepared c client request executed by the selected r endpoint.
#[derive(Debug)]
pub struct ClaudeViaResponses {
    original_request: c::GenerateContentRequestBody,
    target_request: r::GenerateContentRequestBody,
    selected_model: String,
    endpoint: Endpoint,
    identities: GenerationIdentity,
    report: Report,
}
impl ClaudeViaResponses {
    pub fn prepare(
        input: c::GenerateContentRequestBody,
        selected_model: impl Into<String>,
        endpoint: Endpoint,
        mut identities: GenerationIdentity,
    ) -> Result<Self, TransformError> {
        let selected_model = selected_model.into();
        if selected_model.trim().is_empty() {
            return Err(TransformError::missing_metadata("selected_model"));
        }
        let original_request = input.into_declared();
        if original_request.stream == Some(true) {
            return Err(TransformError::unsupported(
                "stream",
                "use the incremental invocation entrypoint",
            ));
        }
        let converted = p::claude_to_responses_request(
            original_request.clone(),
            &selected_model,
            &mut identities.request,
            &identities.request_policy,
        )?;
        Ok(Self {
            original_request,
            target_request: converted.value.into_declared(),
            selected_model,
            endpoint,
            identities,
            report: converted.report,
        })
    }
    /// Restore exact tool aliases and names from declared history/scoped state before mapping.
    pub async fn prepare_with_state<S: crate::capability::StateStore>(
        input: c::GenerateContentRequestBody,

        endpoint: Endpoint,
        identities: GenerationIdentity,
        state: &super::GenerationStateAccess<'_, S>,
    ) -> Result<Self, TransformError> {
        let selected_model = state.target.model.clone();

        let original = input.into_declared();
        let (restored, names) = super::history::claude(original.clone(), state).await?;
        let _ = names;
        let mut prepared = Self::prepare(restored, selected_model, endpoint, identities)?;
        prepared.original_request = original;
        Ok(prepared)
    }
    /// Materialize foreign resources and restore scoped history before preparing the selected endpoint.
    pub async fn prepare_with_capabilities<
        S: crate::capability::StateStore,
        R: crate::capability::ResourceAccess,
    >(
        input: c::GenerateContentRequestBody,

        endpoint: Endpoint,
        identities: GenerationIdentity,
        state: &super::GenerationStateAccess<'_, S>,
        resources: &super::GenerationResources<'_, R>,
    ) -> Result<Self, TransformError> {
        let original = input.into_declared();

        let materialized = resources.claude(original.clone()).await?;
        let mut prepared =
            Self::prepare_with_state(materialized, endpoint, identities, state).await?;
        prepared.original_request = original;
        Ok(prepared)
    }

    pub fn original_request(&self) -> &c::GenerateContentRequestBody {
        &self.original_request
    }
    pub fn target_request(&self) -> &r::GenerateContentRequestBody {
        &self.target_request
    }
    pub fn identities(&self) -> &GenerationIdentity {
        &self.identities
    }
    pub fn report(&self) -> &Report {
        &self.report
    }
    pub fn selected_model(&self) -> &str {
        &self.selected_model
    }
    pub fn convert_response(
        &mut self,
        native: r::GenerateContentResponseBody,
        facts: p::ClaudeRequestContext,
    ) -> Result<Converted<c::GenerateContentResponseBody>, TransformError> {
        let native = native.into_declared();
        p::responses_to_claude_response_with_context(
            native,
            facts,
            &mut self.identities.response,
            &self.identities.response_policy,
        )
    }
    /// Performs one POST. The factual supplement is obtained from the actual decoded response.
    /// Conversion errors keep native bytes/results in caller-owned progress.
    pub async fn invoke<U: Upstream, S: crate::capability::StateStore>(
        &mut self,
        upstream: &U,
        target: &U::Target,
        limits: CodecLimits,
        state: &super::GenerationStateAccess<'_, S>,
        progress: &mut GenerationProgress<r::GenerateContentResponseBody>,
        facts: impl FnOnce(
            &r::GenerateContentResponseBody,
        ) -> Result<p::ClaudeRequestContext, TransformError>,
    ) -> Result<GenerationOutcome<c::GenerateContentResponseBody>, TransformError> {
        transport::bind(
            (&self.endpoint, &self.selected_model),
            &self.identities,
            (&self.original_request, &self.target_request),
            state,
            limits,
            progress,
            false,
        )?;
        if !transport::send(
            upstream,
            target,
            self.endpoint.clone(),
            self.target_request.clone(),
            limits,
            progress,
        )
        .await?
        {
            return Ok(transport::rejected(progress));
        }
        self.recover(limits, state, progress, facts).await
    }
    /// Complete response conversion and pending state writes using the retained result; sends no request.
    pub async fn recover<S: crate::capability::StateStore>(
        &mut self,
        limits: CodecLimits,
        state: &super::GenerationStateAccess<'_, S>,
        progress: &mut GenerationProgress<r::GenerateContentResponseBody>,
        facts: impl FnOnce(
            &r::GenerateContentResponseBody,
        ) -> Result<p::ClaudeRequestContext, TransformError>,
    ) -> Result<GenerationOutcome<c::GenerateContentResponseBody>, TransformError> {
        transport::bind(
            (&self.endpoint, &self.selected_model),
            &self.identities,
            (&self.original_request, &self.target_request),
            state,
            limits,
            progress,
            true,
        )?;
        if progress
            .raw_response
            .as_ref()
            .is_none_or(|r| !r.status.is_success())
        {
            return Err(TransformError::invalid_result(
                "generation.recovery",
                "successful native response required",
            ));
        }
        let native = transport::recover_native(progress, limits)?;
        let facts = facts(&native)?;
        let converted = self.convert_response(native.clone(), facts)?;
        state
            .save_pair(
                &native,
                &converted.value,
                &self.identities.response,
                progress,
            )
            .await?;
        transport::finish(progress, converted, self.report.clone(), limits)
    }
}
/// Effective settings and measured facts absent from the upstream wire response.
#[derive(Debug)]
pub struct ClaudeReturnFacts {
    pub parallel_tool_calls: bool,
    pub tool_choice: r::input::ToolChoice,
    pub prompt_cache_options: Option<r::response::ResponsePromptCacheOptions>,
    pub usage: p::ResponsesUsageFacts,
    pub created_at: i64,
}
/// A prepared r client request executed by the selected c endpoint.
#[derive(Debug)]
pub struct ResponsesViaClaude {
    original_request: r::GenerateContentRequestBody,
    target_request: c::GenerateContentRequestBody,
    selected_model: String,
    endpoint: Endpoint,
    identities: GenerationIdentity,
    report: Report,
}
impl ResponsesViaClaude {
    pub fn prepare(
        input: r::GenerateContentRequestBody,
        selected_model: impl Into<String>,
        endpoint: Endpoint,
        mut identities: GenerationIdentity,
        context: p::ClaudeRequestContext,
    ) -> Result<Self, TransformError> {
        let selected_model = selected_model.into();
        if selected_model.trim().is_empty() {
            return Err(TransformError::missing_metadata("selected_model"));
        }
        let original_request = input.into_declared();
        if original_request.stream.flatten() == Some(true) {
            return Err(TransformError::unsupported(
                "stream",
                "use the incremental invocation entrypoint",
            ));
        }
        let mut converted =
            p::responses_to_claude_request(original_request.clone(), &selected_model, context)?;
        super::claude_chat_ids::claude_request(
            &mut converted.value,
            &mut identities,
            Dialect::OpenAi,
        )?;
        Ok(Self {
            original_request,
            target_request: converted.value.into_declared(),
            selected_model,
            endpoint,
            identities,
            report: converted.report,
        })
    }
    /// Restore exact tool aliases and names from declared history/scoped state before mapping.
    pub async fn prepare_with_state<S: crate::capability::StateStore>(
        input: r::GenerateContentRequestBody,

        endpoint: Endpoint,
        identities: GenerationIdentity,
        state: &super::GenerationStateAccess<'_, S>,
        context: p::ClaudeRequestContext,
    ) -> Result<Self, TransformError> {
        let selected_model = state.target.model.clone();

        let original = input.into_declared();
        let mut lowered = original.clone();
        let mut tool_report = Report::default();
        crate::transform::generate::client_tools::Bindings::for_target(&original, Dialect::Claude)?
            .lower(&mut lowered, &mut tool_report)?;
        let (restored, names) = super::history::responses(lowered.clone(), state).await?;
        let _ = names;
        let mut context = context;
        if context
            .target
            .as_ref()
            .is_some_and(|target| target != &state.target)
        {
            return Err(TransformError::shape(
                "signature.context",
                "caller replay binding conflicts with selected state",
            ));
        }
        let recovered = state.claude_replay(&lowered).await?;
        context.target = recovered.target;
        for (id, piece) in recovered.restored_thinking {
            if context.restored_thinking.contains_key(&id) {
                return Err(TransformError::shape(
                    "signature.context",
                    "duplicate caller and stored native piece",
                ));
            }
            context.restored_thinking.insert(id, piece);
        }
        let mut prepared = Self::prepare(restored, selected_model, endpoint, identities, context)?;
        prepared.original_request = original;
        prepared.report.diagnostics.extend(tool_report.diagnostics);
        Ok(prepared)
    }
    /// Materialize foreign resources and restore scoped history before preparing the selected endpoint.
    pub async fn prepare_with_capabilities<
        S: crate::capability::StateStore,
        R: crate::capability::ResourceAccess,
    >(
        input: r::GenerateContentRequestBody,

        endpoint: Endpoint,
        identities: GenerationIdentity,
        state: &super::GenerationStateAccess<'_, S>,
        resources: &super::GenerationResources<'_, R>,
        context: p::ClaudeRequestContext,
    ) -> Result<Self, TransformError> {
        let original = input.into_declared();

        let materialized = resources.responses(original.clone()).await?;
        let mut prepared =
            Self::prepare_with_state(materialized, endpoint, identities, state, context).await?;
        prepared.original_request = original;
        Ok(prepared)
    }

    pub fn original_request(&self) -> &r::GenerateContentRequestBody {
        &self.original_request
    }
    pub fn target_request(&self) -> &c::GenerateContentRequestBody {
        &self.target_request
    }
    pub fn identities(&self) -> &GenerationIdentity {
        &self.identities
    }
    pub fn report(&self) -> &Report {
        &self.report
    }
    pub fn selected_model(&self) -> &str {
        &self.selected_model
    }
    pub fn convert_response(
        &mut self,
        native: c::GenerateContentResponseBody,
        facts: ClaudeReturnFacts,
    ) -> Result<Converted<r::GenerateContentResponseBody>, TransformError> {
        let native = native.into_declared();
        p::claude_to_responses_response(
            native,
            p::ClaudeResponseContext {
                request: self.original_request.clone(),
                effective_parallel_tool_calls: facts.parallel_tool_calls,
                effective_tool_choice: facts.tool_choice,
                usage: facts.usage,
                created_at: facts.created_at,
                effective_prompt_cache_options: facts.prompt_cache_options,
            },
            &mut self.identities.response,
            &self.identities.response_policy,
        )
    }
    /// Performs one POST. The factual supplement is obtained from the actual decoded response.
    /// Conversion errors keep native bytes/results in caller-owned progress.
    pub async fn invoke<U: Upstream, S: crate::capability::StateStore>(
        &mut self,
        upstream: &U,
        target: &U::Target,
        limits: CodecLimits,
        state: &super::GenerationStateAccess<'_, S>,
        progress: &mut GenerationProgress<c::GenerateContentResponseBody>,
        facts: impl FnOnce(&c::GenerateContentResponseBody) -> Result<ClaudeReturnFacts, TransformError>,
    ) -> Result<GenerationOutcome<r::GenerateContentResponseBody>, TransformError> {
        transport::bind(
            (&self.endpoint, &self.selected_model),
            &self.identities,
            (&self.original_request, &self.target_request),
            state,
            limits,
            progress,
            false,
        )?;
        if !transport::send(
            upstream,
            target,
            self.endpoint.clone(),
            self.target_request.clone(),
            limits,
            progress,
        )
        .await?
        {
            return Ok(transport::rejected(progress));
        }
        self.recover(limits, state, progress, facts).await
    }
    /// Complete response conversion and pending state writes using the retained result; sends no request.
    pub async fn recover<S: crate::capability::StateStore>(
        &mut self,
        limits: CodecLimits,
        state: &super::GenerationStateAccess<'_, S>,
        progress: &mut GenerationProgress<c::GenerateContentResponseBody>,
        facts: impl FnOnce(&c::GenerateContentResponseBody) -> Result<ClaudeReturnFacts, TransformError>,
    ) -> Result<GenerationOutcome<r::GenerateContentResponseBody>, TransformError> {
        transport::bind(
            (&self.endpoint, &self.selected_model),
            &self.identities,
            (&self.original_request, &self.target_request),
            state,
            limits,
            progress,
            true,
        )?;
        if progress
            .raw_response
            .as_ref()
            .is_none_or(|r| !r.status.is_success())
        {
            return Err(TransformError::invalid_result(
                "generation.recovery",
                "successful native response required",
            ));
        }
        let native = transport::recover_native(progress, limits)?;
        let facts = facts(&native)?;
        let converted = self.convert_response(native.clone(), facts)?;
        state
            .save_pair(
                &native,
                &converted.value,
                &self.identities.response,
                progress,
            )
            .await?;
        transport::finish(progress, converted, self.report.clone(), limits)
    }
}

mod synthesis;

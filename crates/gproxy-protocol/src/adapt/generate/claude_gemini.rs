use super::{Endpoint, GenerationIdentity, GenerationOutcome, GenerationProgress, transport};
use crate::transform::generate::claude_gemini as p;
use crate::wire::claude::generate_content as c;
use crate::wire::gemini as g;
use crate::{
    Dialect,
    capability::Upstream,
    codec::CodecLimits,
    transform::{Converted, Report, TransformError},
    wire::DeclaredFields,
};
/// A prepared c client request executed by the selected g endpoint.
#[derive(Debug)]
pub struct ClaudeViaGemini {
    original_request: c::GenerateContentRequestBody,
    target_request: g::GenerateContentRequestBody,
    selected_model: String,
    endpoint: Endpoint,
    identities: GenerationIdentity,
    report: Report,
}
impl ClaudeViaGemini {
    pub fn prepare(
        input: c::GenerateContentRequestBody,
        selected_model: impl Into<String>,
        endpoint: Endpoint,
        mut identities: GenerationIdentity,
        context: p::ClaudeGeminiRequestContext,
    ) -> Result<Self, TransformError> {
        identities.validate(Dialect::Claude, Dialect::Gemini)?;
        endpoint.validate()?;
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
        let converted = p::claude_to_gemini_request(
            original_request.clone(),
            &selected_model,
            context,
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
        selected_model: impl Into<String>,
        endpoint: Endpoint,
        identities: GenerationIdentity,
        state: &super::GenerationStateAccess<'_, S>,
        context: p::ClaudeGeminiRequestContext,
    ) -> Result<Self, TransformError> {
        let selected_model = selected_model.into();
        state.validate_target(Dialect::Gemini, &selected_model)?;
        let original = input.into_declared();
        let (restored, names) = super::history::claude(original.clone(), state).await?;
        let mut context = context;
        super::history::merge_names(&mut context.tool_names, names.names)?;
        let mut prepared = Self::prepare(restored, selected_model, endpoint, identities, context)?;
        state
            .restore_gemini_tool_parts(
                &mut prepared.target_request,
                &prepared.identities.request,
                &prepared.identities.request_policy,
            )
            .await?;
        prepared.original_request = original;
        Ok(prepared)
    }
    /// Materialize foreign resources and restore scoped history before preparing the selected endpoint.
    pub async fn prepare_with_capabilities<
        S: crate::capability::StateStore,
        R: crate::capability::ResourceAccess,
    >(
        input: c::GenerateContentRequestBody,
        selected_model: impl Into<String>,
        endpoint: Endpoint,
        identities: GenerationIdentity,
        state: &super::GenerationStateAccess<'_, S>,
        resources: &super::GenerationResources<'_, R>,
        context: p::ClaudeGeminiRequestContext,
    ) -> Result<Self, TransformError> {
        let original = input.into_declared();
        let selected_model = selected_model.into();
        state.validate_target(identities.request_policy.dialect, &selected_model)?;
        endpoint.validate()?;
        let materialized = resources.claude(original.clone()).await?;
        let mut prepared = Self::prepare_with_state(
            materialized,
            selected_model,
            endpoint,
            identities,
            state,
            context,
        )
        .await?;
        prepared.original_request = original;
        Ok(prepared)
    }

    pub fn original_request(&self) -> &c::GenerateContentRequestBody {
        &self.original_request
    }
    pub fn target_request(&self) -> &g::GenerateContentRequestBody {
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
        native: g::GenerateContentResponseBody,
        facts: p::ClaudeGeminiUsageFacts,
    ) -> Result<Converted<c::GenerateContentResponseBody>, TransformError> {
        let native = native.into_declared();
        p::gemini_to_claude_response(
            native,
            Some(self.selected_model.clone()),
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
        progress: &mut GenerationProgress<g::GenerateContentResponseBody>,
        facts: impl FnOnce(
            &g::GenerateContentResponseBody,
        ) -> Result<p::ClaudeGeminiUsageFacts, TransformError>,
    ) -> Result<GenerationOutcome<c::GenerateContentResponseBody>, TransformError> {
        state.validate_target(self.identities.request_policy.dialect, &self.selected_model)?;
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
        progress: &mut GenerationProgress<g::GenerateContentResponseBody>,
        facts: impl FnOnce(
            &g::GenerateContentResponseBody,
        ) -> Result<p::ClaudeGeminiUsageFacts, TransformError>,
    ) -> Result<GenerationOutcome<c::GenerateContentResponseBody>, TransformError> {
        state.validate_target(self.identities.request_policy.dialect, &self.selected_model)?;
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
/// A prepared g client request executed by the selected c endpoint.
#[derive(Debug)]
pub struct GeminiViaClaude {
    original_request: g::GenerateContentRequestBody,
    target_request: c::GenerateContentRequestBody,
    selected_model: String,
    endpoint: Endpoint,
    identities: GenerationIdentity,
    report: Report,
}
impl GeminiViaClaude {
    pub fn prepare(
        input: g::GenerateContentRequestBody,
        selected_model: impl Into<String>,
        endpoint: Endpoint,
        mut identities: GenerationIdentity,
        max_tokens: Option<i64>,
    ) -> Result<Self, TransformError> {
        identities.validate(Dialect::Gemini, Dialect::Claude)?;
        endpoint.validate()?;
        let selected_model = selected_model.into();
        if selected_model.trim().is_empty() {
            return Err(TransformError::missing_metadata("selected_model"));
        }
        let original_request = input.into_declared();

        let converted = p::gemini_to_claude_request(
            original_request.clone(),
            &selected_model,
            max_tokens,
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
        input: g::GenerateContentRequestBody,
        selected_model: impl Into<String>,
        endpoint: Endpoint,
        identities: GenerationIdentity,
        state: &super::GenerationStateAccess<'_, S>,
        max_tokens: Option<i64>,
    ) -> Result<Self, TransformError> {
        let selected_model = selected_model.into();
        state.validate_target(Dialect::Claude, &selected_model)?;
        let original = input.into_declared();
        let (restored, names) = super::history::gemini(original.clone(), state).await?;
        let _ = names;
        let mut prepared =
            Self::prepare(restored, selected_model, endpoint, identities, max_tokens)?;
        prepared.original_request = original;
        Ok(prepared)
    }
    /// Materialize foreign resources and restore scoped history before preparing the selected endpoint.
    pub async fn prepare_with_capabilities<
        S: crate::capability::StateStore,
        R: crate::capability::ResourceAccess,
    >(
        input: g::GenerateContentRequestBody,
        selected_model: impl Into<String>,
        endpoint: Endpoint,
        identities: GenerationIdentity,
        state: &super::GenerationStateAccess<'_, S>,
        resources: &super::GenerationResources<'_, R>,
        max_tokens: Option<i64>,
    ) -> Result<Self, TransformError> {
        let original = input.into_declared();
        let selected_model = selected_model.into();
        state.validate_target(identities.request_policy.dialect, &selected_model)?;
        endpoint.validate()?;
        let materialized = resources.gemini(original.clone()).await?;
        let mut prepared = Self::prepare_with_state(
            materialized,
            selected_model,
            endpoint,
            identities,
            state,
            max_tokens,
        )
        .await?;
        prepared.original_request = original;
        Ok(prepared)
    }

    pub fn original_request(&self) -> &g::GenerateContentRequestBody {
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
        facts: p::ClaudeGeminiUsageFacts,
    ) -> Result<Converted<g::GenerateContentResponseBody>, TransformError> {
        let native = native.into_declared();
        p::claude_to_gemini_response(
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
        progress: &mut GenerationProgress<c::GenerateContentResponseBody>,
        facts: impl FnOnce(
            &c::GenerateContentResponseBody,
        ) -> Result<p::ClaudeGeminiUsageFacts, TransformError>,
    ) -> Result<GenerationOutcome<g::GenerateContentResponseBody>, TransformError> {
        state.validate_target(self.identities.request_policy.dialect, &self.selected_model)?;
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
        facts: impl FnOnce(
            &c::GenerateContentResponseBody,
        ) -> Result<p::ClaudeGeminiUsageFacts, TransformError>,
    ) -> Result<GenerationOutcome<g::GenerateContentResponseBody>, TransformError> {
        state.validate_target(self.identities.request_policy.dialect, &self.selected_model)?;
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

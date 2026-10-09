use super::{Endpoint, GenerationIdentity, GenerationOutcome, GenerationProgress, transport};
use crate::transform::generate::claude_chat as p;
use crate::wire::claude::generate_content as c;
use crate::wire::openai::chat as h;
use crate::{
    Dialect,
    capability::Upstream,
    codec::CodecLimits,
    transform::{Converted, Report, TransformError},
    wire::DeclaredFields,
};

/// A prepared Chat Completions client request executed by the selected Claude endpoint.
#[derive(Debug)]
pub struct ChatViaClaude {
    original_request: h::GenerateContentRequestBody,
    target_request: c::GenerateContentRequestBody,
    selected_model: String,
    endpoint: Endpoint,
    identities: GenerationIdentity,
    report: Report,
}

impl ChatViaClaude {
    pub fn prepare(
        input: h::GenerateContentRequestBody,
        selected_model: impl Into<String>,
        endpoint: Endpoint,
        mut identities: GenerationIdentity,
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
        let mut converted = p::openai_to_claude(&original_request, &selected_model)?;
        super::claude_chat_ids::claude_request(
            &mut converted.value,
            &mut identities,
            Dialect::OpenAiChat,
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
        input: h::GenerateContentRequestBody,
        endpoint: Endpoint,
        identities: GenerationIdentity,
        state: &super::GenerationStateAccess<'_, S>,
    ) -> Result<Self, TransformError> {
        let selected_model = state.target.model.clone();
        let original = input.into_declared();
        let (restored, names) = super::history::chat(original.clone(), state).await?;
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
        input: h::GenerateContentRequestBody,
        endpoint: Endpoint,
        identities: GenerationIdentity,
        state: &super::GenerationStateAccess<'_, S>,
        resources: &super::GenerationResources<'_, R>,
    ) -> Result<Self, TransformError> {
        let original = input.into_declared();
        let materialized = resources.chat(original.clone()).await?;
        let mut prepared =
            Self::prepare_with_state(materialized, endpoint, identities, state).await?;
        prepared.original_request = original;
        Ok(prepared)
    }

    pub fn original_request(&self) -> &h::GenerateContentRequestBody {
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
        facts: p::ResponseSupplement,
    ) -> Result<Converted<h::GenerateContentResponseBody>, TransformError> {
        let native = native.into_declared();
        let mut converted = p::claude_response_to_openai(&native, &self.selected_model, &facts)?;
        super::claude_chat_ids::chat_response(&mut converted.value, &mut self.identities)?;
        Ok(converted)
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
        ) -> Result<p::ResponseSupplement, TransformError>,
    ) -> Result<GenerationOutcome<h::GenerateContentResponseBody>, TransformError> {
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
        ) -> Result<p::ResponseSupplement, TransformError>,
    ) -> Result<GenerationOutcome<h::GenerateContentResponseBody>, TransformError> {
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
        let converted = self.convert_response(native, facts)?;
        transport::finish(progress, converted, self.report.clone(), limits)
    }
}

/// A prepared Claude client request executed by the selected Chat Completions endpoint.
#[derive(Debug)]
pub struct ClaudeViaChat {
    original_request: c::GenerateContentRequestBody,
    target_request: h::GenerateContentRequestBody,
    selected_model: String,
    endpoint: Endpoint,
    identities: GenerationIdentity,
    report: Report,
}

impl ClaudeViaChat {
    pub fn prepare(
        input: c::GenerateContentRequestBody,
        selected_model: impl Into<String>,
        endpoint: Endpoint,
        identities: GenerationIdentity,
    ) -> Result<Self, TransformError> {
        Self::prepare_with_replay(
            input,
            selected_model,
            endpoint,
            identities,
            &Default::default(),
        )
    }
    fn prepare_with_replay(
        input: c::GenerateContentRequestBody,
        selected_model: impl Into<String>,
        endpoint: Endpoint,
        mut identities: GenerationIdentity,
        replay: &super::GenerationToolReplay,
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
        let mut converted = p::claude_to_openai(&original_request, &selected_model)?;
        super::legacy_chat::restore(&mut converted.value, replay, &mut converted.report)?;
        super::claude_chat_ids::chat_request(
            &mut converted.value,
            &mut identities,
            Dialect::Claude,
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
        let mut prepared =
            Self::prepare_with_replay(restored, selected_model, endpoint, identities, &names)?;
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
    pub fn target_request(&self) -> &h::GenerateContentRequestBody {
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
        native: h::GenerateContentResponseBody,
        facts: p::ResponseSupplement,
    ) -> Result<Converted<c::GenerateContentResponseBody>, TransformError> {
        let native = native.into_declared();
        let mut converted = p::openai_response_to_claude(&native, &self.selected_model, &facts)?;
        super::claude_chat_ids::claude_response(&mut converted.value, &mut self.identities)?;
        Ok(converted)
    }
    /// Performs one POST. The factual supplement is obtained from the actual decoded response.
    /// Conversion errors keep native bytes/results in caller-owned progress.
    pub async fn invoke<U: Upstream, S: crate::capability::StateStore>(
        &mut self,
        upstream: &U,
        target: &U::Target,
        limits: CodecLimits,
        state: &super::GenerationStateAccess<'_, S>,
        progress: &mut GenerationProgress<h::GenerateContentResponseBody>,
        facts: impl FnOnce(
            &h::GenerateContentResponseBody,
        ) -> Result<p::ResponseSupplement, TransformError>,
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
        progress: &mut GenerationProgress<h::GenerateContentResponseBody>,
        facts: impl FnOnce(
            &h::GenerateContentResponseBody,
        ) -> Result<p::ResponseSupplement, TransformError>,
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
        let converted = self.convert_response(native, facts)?;
        transport::finish(progress, converted, self.report.clone(), limits)
    }
}

super::prepared::prepared_generation!(ChatViaClaude, h::GenerateContentRequestBody => c::GenerateContentRequestBody);
super::prepared::stream_synthesis!(ChatViaClaude, h::GenerateContentRequestBody);
super::prepared::prepared_generation!(ClaudeViaChat, c::GenerateContentRequestBody => h::GenerateContentRequestBody);
super::prepared::stream_synthesis!(ClaudeViaChat, c::GenerateContentRequestBody);

use super::{Endpoint, GenerationIdentity, GenerationOutcome, GenerationProgress, transport};
use crate::transform::generate::gemini_chat as p;
use crate::wire::gemini as g;
use crate::wire::openai::chat as h;
use crate::{
    Dialect,
    capability::Upstream,
    codec::CodecLimits,
    transform::{Converted, Report, TransformError},
    wire::DeclaredFields,
};

/// A prepared Chat Completions client request executed by the selected Gemini endpoint.
#[derive(Debug)]
pub struct ChatViaGemini {
    original_request: h::GenerateContentRequestBody,
    target_request: g::GenerateContentRequestBody,
    selected_model: String,
    endpoint: Endpoint,
    identities: GenerationIdentity,
    report: Report,
}

impl ChatViaGemini {
    pub fn prepare(
        input: h::GenerateContentRequestBody,
        selected_model: impl Into<String>,
        endpoint: Endpoint,
        mut identities: GenerationIdentity,
        function_names: &std::collections::BTreeMap<String, String>,
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
            p::openai_to_gemini_request(&original_request, &selected_model, function_names)?;
        super::request_ids::gemini_request(
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
        function_names: &std::collections::BTreeMap<String, String>,
    ) -> Result<Self, TransformError> {
        let selected_model = state.target.model.clone();
        let original = input.into_declared();
        let (restored, names) = super::history::chat(original.clone(), state).await?;
        let mut merged_names = function_names.clone();
        super::history::merge_names(&mut merged_names, names.names)?;
        let mut prepared = Self::prepare(
            restored,
            selected_model,
            endpoint,
            identities,
            &merged_names,
        )?;
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
        function_names: &std::collections::BTreeMap<String, String>,
    ) -> Result<Self, TransformError> {
        let original = input.into_declared();
        let materialized = resources.chat(original.clone()).await?;
        let mut prepared =
            Self::prepare_with_state(materialized, endpoint, identities, state, function_names)
                .await?;
        prepared.original_request = original;
        Ok(prepared)
    }

    pub fn original_request(&self) -> &h::GenerateContentRequestBody {
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
        facts: p::GeminiChatResponseSupplement,
    ) -> Result<Converted<h::GenerateContentResponseBody>, TransformError> {
        let native = native.into_declared();
        p::gemini_to_openai_response(
            native,
            &self.selected_model,
            &facts,
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
        ) -> Result<p::GeminiChatResponseSupplement, TransformError>,
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
        progress: &mut GenerationProgress<g::GenerateContentResponseBody>,
        facts: impl FnOnce(
            &g::GenerateContentResponseBody,
        ) -> Result<p::GeminiChatResponseSupplement, TransformError>,
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

/// A prepared Gemini client request executed by the selected Chat Completions endpoint.
#[derive(Debug)]
pub struct GeminiViaChat {
    original_request: g::GenerateContentRequestBody,
    target_request: h::GenerateContentRequestBody,
    selected_model: String,
    endpoint: Endpoint,
    identities: GenerationIdentity,
    report: Report,
}

impl GeminiViaChat {
    pub fn prepare(
        input: g::GenerateContentRequestBody,
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
        input: g::GenerateContentRequestBody,
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

        let converted = p::gemini_to_openai_request_with_calls(
            original_request.clone(),
            &selected_model,
            &replay.legacy_chat_calls(),
            &replay.original_chat_calls(),
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
        endpoint: Endpoint,
        identities: GenerationIdentity,
        state: &super::GenerationStateAccess<'_, S>,
    ) -> Result<Self, TransformError> {
        let selected_model = state.target.model.clone();
        let original = input.into_declared();
        let (restored, names) = super::history::gemini(original.clone(), state).await?;
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
        input: g::GenerateContentRequestBody,
        endpoint: Endpoint,
        identities: GenerationIdentity,
        state: &super::GenerationStateAccess<'_, S>,
        resources: &super::GenerationResources<'_, R>,
    ) -> Result<Self, TransformError> {
        let original = input.into_declared();
        let materialized = resources.gemini(original.clone()).await?;
        let mut prepared =
            Self::prepare_with_state(materialized, endpoint, identities, state).await?;
        prepared.original_request = original;
        Ok(prepared)
    }

    pub fn original_request(&self) -> &g::GenerateContentRequestBody {
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
        _facts: (),
    ) -> Result<Converted<g::GenerateContentResponseBody>, TransformError> {
        let native = native.into_declared();
        let mut converted = p::openai_to_gemini_response(&native)?;
        super::request_ids::gemini_response(&mut converted.value, &mut self.identities, &native)?;
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
        facts: impl FnOnce(&h::GenerateContentResponseBody) -> Result<(), TransformError>,
    ) -> Result<GenerationOutcome<g::GenerateContentResponseBody>, TransformError> {
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
        facts: impl FnOnce(&h::GenerateContentResponseBody) -> Result<(), TransformError>,
    ) -> Result<GenerationOutcome<g::GenerateContentResponseBody>, TransformError> {
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
        facts(&native)?;
        let converted = self.convert_response(native, ())?;
        transport::finish(progress, converted, self.report.clone(), limits)
    }
}

super::prepared::prepared_generation!(ChatViaGemini, h::GenerateContentRequestBody => g::GenerateContentRequestBody);
super::prepared::stream_synthesis!(ChatViaGemini, h::GenerateContentRequestBody, function_names: &std::collections::BTreeMap<String, String>);
super::prepared::prepared_generation!(GeminiViaChat, g::GenerateContentRequestBody => h::GenerateContentRequestBody);
super::prepared::stream_synthesis!(GeminiViaChat, g::GenerateContentRequestBody);

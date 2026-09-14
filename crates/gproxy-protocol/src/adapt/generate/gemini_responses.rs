use super::{Endpoint, GenerationIdentity, GenerationOutcome, GenerationProgress, transport};
use crate::transform::generate::gemini_responses as p;
use crate::wire::gemini as g;
use crate::wire::openai::responses as r;
use crate::{
    Dialect,
    capability::Upstream,
    codec::CodecLimits,
    transform::{Converted, Report, TransformError},
    wire::DeclaredFields,
};
/// A prepared g client request executed by the selected r endpoint.
#[derive(Debug)]
pub struct GeminiViaResponses {
    original_request: g::GenerateContentRequestBody,
    target_request: r::GenerateContentRequestBody,
    selected_model: String,
    endpoint: Endpoint,
    identities: GenerationIdentity,
    report: Report,
    signed_ids: super::request_ids::SignedToolBindings,
}
impl GeminiViaResponses {
    pub fn prepare(
        input: g::GenerateContentRequestBody,
        selected_model: impl Into<String>,
        endpoint: Endpoint,
        mut identities: GenerationIdentity,
    ) -> Result<Self, TransformError> {
        identities.validate(Dialect::Gemini, Dialect::OpenAi)?;
        endpoint.validate()?;
        let selected_model = selected_model.into();
        if selected_model.trim().is_empty() {
            return Err(TransformError::missing_metadata("selected_model"));
        }
        let original_request = input.into_declared();

        let converted = p::gemini_to_responses_request(
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
            signed_ids: Default::default(),
        })
    }
    /// Restore exact tool aliases and names from declared history/scoped state before mapping.
    pub async fn prepare_with_state<S: crate::capability::StateStore>(
        input: g::GenerateContentRequestBody,
        selected_model: impl Into<String>,
        endpoint: Endpoint,
        identities: GenerationIdentity,
        state: &super::GenerationStateAccess<'_, S>,
    ) -> Result<Self, TransformError> {
        let selected_model = selected_model.into();
        state.validate_target(Dialect::OpenAi, &selected_model)?;
        let original = input.into_declared();
        let (restored, names) = super::history::gemini(original.clone(), state).await?;
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
        input: g::GenerateContentRequestBody,
        selected_model: impl Into<String>,
        endpoint: Endpoint,
        identities: GenerationIdentity,
        state: &super::GenerationStateAccess<'_, S>,
        resources: &super::GenerationResources<'_, R>,
    ) -> Result<Self, TransformError> {
        let original = input.into_declared();
        let selected_model = selected_model.into();
        state.validate_target(identities.request_policy.dialect, &selected_model)?;
        endpoint.validate()?;
        let mut materialized = resources.gemini(original.clone()).await?;
        if let Some(image) = materialized
            .generation_config
            .as_mut()
            .and_then(|v| v.response_format.as_mut())
            .and_then(|v| v.image.as_mut())
            && image.delivery == Some(g::Delivery::Uri)
        {
            image.delivery = Some(g::Delivery::Inline);
        }
        let mut prepared =
            Self::prepare_with_state(materialized, selected_model, endpoint, identities, state)
                .await?;
        prepared.original_request = original;
        Ok(prepared)
    }

    pub fn original_request(&self) -> &g::GenerateContentRequestBody {
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
    pub(super) fn signed_tool_bindings(&self) -> &super::request_ids::SignedToolBindings {
        &self.signed_ids
    }
    pub fn selected_model(&self) -> &str {
        &self.selected_model
    }
    pub fn convert_response(
        &mut self,
        native: r::GenerateContentResponseBody,
        facts: p::GeminiReplayContext,
    ) -> Result<Converted<g::GenerateContentResponseBody>, TransformError> {
        if self
            .original_request
            .generation_config
            .as_ref()
            .and_then(|v| v.response_format.as_ref())
            .and_then(|v| v.image.as_ref())
            .and_then(|v| v.delivery.as_ref())
            == Some(&g::Delivery::Uri)
        {
            return Err(TransformError::new(
                crate::transform::TransformErrorKind::MissingState,
                "image.delivery",
                "URI output requires the image resource invocation adapter",
            ));
        }
        self.convert_response_inline(native, facts)
    }
    pub(super) fn convert_response_inline(
        &mut self,
        native: r::GenerateContentResponseBody,
        facts: p::GeminiReplayContext,
    ) -> Result<Converted<g::GenerateContentResponseBody>, TransformError> {
        let native = native.into_declared();
        let modalities = self
            .original_request
            .generation_config
            .as_ref()
            .and_then(|v| v.response_modalities.as_deref());
        let mut converted =
            p::responses_to_gemini_response_with_modalities(native.clone(), facts, modalities)?;
        if self
            .original_request
            .generation_config
            .as_ref()
            .and_then(|v| v.response_format.as_ref())
            .and_then(|v| v.image.as_ref())
            .and_then(|v| v.mime_type.as_ref())
            == Some(&g::ImageMimeType::ImageJpeg)
        {
            for blob in converted
                .value
                .candidates
                .iter()
                .flatten()
                .filter_map(|c| c.content.as_ref())
                .flat_map(|c| c.parts.iter().flatten())
                .filter_map(|p| p.inline_data.as_ref())
            {
                if blob.mime_type != "image/jpeg" {
                    return Err(TransformError::invalid_result(
                        "image.mime",
                        "actual image does not match requested JPEG output",
                    ));
                }
            }
        }
        self.signed_ids = super::request_ids::gemini_response(
            &mut converted.value,
            &mut self.identities,
            &native,
        )?;
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
        progress: &mut GenerationProgress<r::GenerateContentResponseBody>,
        facts: impl FnOnce(
            &r::GenerateContentResponseBody,
        ) -> Result<p::GeminiReplayContext, TransformError>,
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
        progress: &mut GenerationProgress<r::GenerateContentResponseBody>,
        facts: impl FnOnce(
            &r::GenerateContentResponseBody,
        ) -> Result<p::GeminiReplayContext, TransformError>,
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
            .save_pair_with_bound_ids(
                &native,
                &converted.value,
                &self.identities.response,
                self.signed_tool_bindings(),
                progress,
            )
            .await?;
        transport::finish(progress, converted, self.report.clone(), limits)
    }
}
/// Effective settings and measured facts absent from the upstream wire response.
#[derive(Debug)]
pub struct GeminiReturnFacts {
    pub parallel_tool_calls: bool,
    pub tool_choice: r::input::ToolChoice,
    pub prompt_cache_options: Option<r::response::ResponsePromptCacheOptions>,
    pub usage: p::GeminiUsageFacts,
    pub created_at: i64,
}
/// A prepared r client request executed by the selected g endpoint.
#[derive(Debug)]
pub struct ResponsesViaGemini {
    original_request: r::GenerateContentRequestBody,
    target_request: g::GenerateContentRequestBody,
    selected_model: String,
    endpoint: Endpoint,
    identities: GenerationIdentity,
    report: Report,
}
impl ResponsesViaGemini {
    pub fn prepare(
        input: r::GenerateContentRequestBody,
        selected_model: impl Into<String>,
        endpoint: Endpoint,
        mut identities: GenerationIdentity,
        context: p::GeminiReplayContext,
    ) -> Result<Self, TransformError> {
        identities.validate(Dialect::OpenAi, Dialect::Gemini)?;
        endpoint.validate()?;
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
            p::responses_to_gemini_request(original_request.clone(), &selected_model, context)?;
        super::request_ids::gemini_request(&mut converted.value, &mut identities, Dialect::OpenAi)?;
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
        selected_model: impl Into<String>,
        endpoint: Endpoint,
        identities: GenerationIdentity,
        state: &super::GenerationStateAccess<'_, S>,
        context: p::GeminiReplayContext,
    ) -> Result<Self, TransformError> {
        let selected_model = selected_model.into();
        state.validate_target(Dialect::Gemini, &selected_model)?;
        let original = input.into_declared();
        let mut lowered = original.clone();
        let mut tool_report = Report::default();
        crate::transform::generate::client_tools::Bindings::for_target(&original, Dialect::Gemini)?
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
        let recovered = state.gemini_replay(&lowered).await?;
        context.target = recovered.target;
        for (id, piece) in recovered.parts {
            if context.parts.contains_key(&id) {
                return Err(TransformError::shape(
                    "signature.context",
                    "duplicate caller and stored native piece",
                ));
            }
            context.parts.insert(id, piece);
        }
        for (id, image) in recovered.image_files {
            if context.parts.contains_key(&id) || context.image_files.insert(id, image).is_some() {
                return Err(TransformError::shape(
                    "signature.context",
                    "duplicate caller and stored image proof",
                ));
            }
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
        selected_model: impl Into<String>,
        endpoint: Endpoint,
        identities: GenerationIdentity,
        state: &super::GenerationStateAccess<'_, S>,
        resources: &super::GenerationResources<'_, R>,
        context: p::GeminiReplayContext,
    ) -> Result<Self, TransformError> {
        let original = input.into_declared();
        let selected_model = selected_model.into();
        state.validate_target(identities.request_policy.dialect, &selected_model)?;
        endpoint.validate()?;
        let materialized = resources.responses(original.clone()).await?;
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

    pub fn original_request(&self) -> &r::GenerateContentRequestBody {
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
        facts: GeminiReturnFacts,
    ) -> Result<Converted<r::GenerateContentResponseBody>, TransformError> {
        let native = native.into_declared();
        p::gemini_to_responses_response(
            native,
            p::GeminiResponseContext {
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
        progress: &mut GenerationProgress<g::GenerateContentResponseBody>,
        facts: impl FnOnce(&g::GenerateContentResponseBody) -> Result<GeminiReturnFacts, TransformError>,
    ) -> Result<GenerationOutcome<r::GenerateContentResponseBody>, TransformError> {
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
        facts: impl FnOnce(&g::GenerateContentResponseBody) -> Result<GeminiReturnFacts, TransformError>,
    ) -> Result<GenerationOutcome<r::GenerateContentResponseBody>, TransformError> {
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

mod image_resources;

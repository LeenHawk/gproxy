use super::*;
impl GeminiViaResponses {
    /// URI delivery is handled by actual publication after generation. Only the
    /// mapped request uses inline delivery; original_request remains unchanged.
    pub async fn prepare_with_image_resources<S: StateStore, R: ResourceAccess>(
        input: g::GenerateContentRequestBody,

        endpoint: Endpoint,
        identities: GenerationIdentity,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
    ) -> Result<Self, TransformError> {
        let original = input.into_declared();
        let mut prepared = Self::prepare_with_capabilities(
            inline_request(original.clone()),
            endpoint,
            identities,
            state,
            resources,
        )
        .await?;
        prepared.original_request = original;
        Ok(prepared)
    }
    /// Convert and publish requested image URLs before persisting identities and
    /// returning any client result. The original native response stays separate.
    pub async fn convert_response_with_image_resources<S: StateStore, R: ResourceAccess>(
        &mut self,
        native: r::GenerateContentResponseBody,
        facts: p::GeminiReplayContext,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
        progress: &mut ImageResourceProgress<r::GenerateContentResponseBody, R::PublishedHandle>,
    ) -> Result<Converted<g::GenerateContentResponseBody>, TransformError> {
        let native = native.into_declared();
        let mut converted = self.convert_response_inline(native.clone(), facts)?;
        if wants_uri(&self.original_request) {
            converted.value = progress
                .publications
                .publish(
                    &converted.value,
                    self.identities.response.namespace(),
                    state.expires_at,
                    resources,
                )
                .await?;
        }
        state
            .save_pair_with_bound_ids(
                &native,
                &converted.value,
                &self.identities.response,
                self.signed_tool_bindings(),
                &mut progress.generation,
            )
            .await?;
        Ok(converted)
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn invoke_with_image_resources<U: Upstream, S: StateStore, R: ResourceAccess>(
        &mut self,
        upstream: &U,
        target: &U::Target,
        limits: CodecLimits,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
        progress: &mut ImageResourceProgress<r::GenerateContentResponseBody, R::PublishedHandle>,
        facts: impl FnOnce(
            &r::GenerateContentResponseBody,
        ) -> Result<p::GeminiReplayContext, TransformError>,
    ) -> Result<GenerationOutcome<g::GenerateContentResponseBody>, TransformError> {
        transport::bind(
            (&self.endpoint, &self.selected_model),
            &self.identities,
            (&self.original_request, &self.target_request),
            state,
            limits,
            &mut progress.generation,
            false,
        )?;
        if !transport::send(
            upstream,
            target,
            self.endpoint.clone(),
            self.target_request.clone(),
            limits,
            &mut progress.generation,
        )
        .await?
        {
            return Ok(transport::rejected(&progress.generation));
        }
        self.recover_with_image_resources(limits, state, resources, progress, facts)
            .await
    }
    pub async fn recover_with_image_resources<S: StateStore, R: ResourceAccess>(
        &mut self,
        limits: CodecLimits,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
        progress: &mut ImageResourceProgress<r::GenerateContentResponseBody, R::PublishedHandle>,
        facts: impl FnOnce(
            &r::GenerateContentResponseBody,
        ) -> Result<p::GeminiReplayContext, TransformError>,
    ) -> Result<GenerationOutcome<g::GenerateContentResponseBody>, TransformError> {
        transport::bind(
            (&self.endpoint, &self.selected_model),
            &self.identities,
            (&self.original_request, &self.target_request),
            state,
            limits,
            &mut progress.generation,
            true,
        )?;
        if progress
            .generation
            .raw_response
            .as_ref()
            .is_none_or(|response| !response.status.is_success())
        {
            return Err(TransformError::invalid_result(
                "generation.recovery",
                "successful native response required",
            ));
        }
        let native = transport::recover_native(&mut progress.generation, limits)?;
        let facts = facts(&native)?;
        let converted = self
            .convert_response_with_image_resources(native, facts, state, resources, progress)
            .await?;
        transport::finish(
            &mut progress.generation,
            converted,
            self.report.clone(),
            limits,
        )
    }
}

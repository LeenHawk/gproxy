use super::*;

impl ResponsesViaGemini {
    /// Materialize actual native fileData images, convert a separate view, then
    /// persist original identities and signed image proofs before returning.
    pub async fn convert_response_with_image_resources<S: StateStore, R: ResourceAccess>(
        &mut self,
        native: g::GenerateContentResponseBody,
        facts: GeminiReturnFacts,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
        progress: &mut ImageResourceProgress<g::GenerateContentResponseBody, R::PublishedHandle>,
    ) -> Result<Converted<r::GenerateContentResponseBody>, TransformError> {
        let native = native.into_declared();
        let view = progress.reads.materialize(&native, resources).await?;
        let converted = self.convert_response(view, facts)?;
        state
            .save_pair(
                &native,
                &converted.value,
                &self.identities.response,
                &mut progress.generation,
            )
            .await?;
        let images: Vec<_> = progress.reads.images().cloned().collect();
        state
            .save_file_image_proofs(
                &images,
                &converted.value,
                &self.identities.response,
                &mut progress.proofs,
            )
            .await?;
        Ok(converted)
    }
    /// One buffered generation POST followed by scoped image reads. Recovery
    /// retains the native result and never repeats the generation request.
    #[allow(clippy::too_many_arguments)]
    pub async fn invoke_with_image_resources<U: Upstream, S: StateStore, R: ResourceAccess>(
        &mut self,
        upstream: &U,
        target: &U::Target,
        limits: CodecLimits,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
        progress: &mut ImageResourceProgress<g::GenerateContentResponseBody, R::PublishedHandle>,
        facts: impl FnOnce(&g::GenerateContentResponseBody) -> Result<GeminiReturnFacts, TransformError>,
    ) -> Result<GenerationOutcome<r::GenerateContentResponseBody>, TransformError> {
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
        progress: &mut ImageResourceProgress<g::GenerateContentResponseBody, R::PublishedHandle>,
        facts: impl FnOnce(&g::GenerateContentResponseBody) -> Result<GeminiReturnFacts, TransformError>,
    ) -> Result<GenerationOutcome<r::GenerateContentResponseBody>, TransformError> {
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

use super::*;

impl GeminiViaResponses {
    /// Prepare a buffered upstream result for later native stream synthesis,
    /// retaining the original client stream flag and all declared controls.
    pub async fn prepare_for_stream_synthesis<S: crate::capability::StateStore>(
        input: g::GenerateContentRequestBody,
        endpoint: Endpoint,
        identities: GenerationIdentity,
        state: &super::super::GenerationStateAccess<'_, S>,
    ) -> Result<Self, TransformError> {
        let original = input.into_declared();
        let buffered = original.clone();
        let mut prepared = Self::prepare_with_state(buffered, endpoint, identities, state).await?;
        prepared.original_request = original;
        prepared.target_request.stream = Some(Some(false));
        Ok(prepared)
    }
    /// Prepare a buffered upstream result for later native stream synthesis,
    /// retaining the original client stream flag and all declared controls.
    pub async fn prepare_for_stream_synthesis_with_capabilities<
        S: crate::capability::StateStore,
        R: crate::capability::ResourceAccess,
    >(
        input: g::GenerateContentRequestBody,
        endpoint: Endpoint,
        identities: GenerationIdentity,
        state: &super::super::GenerationStateAccess<'_, S>,
        resources: &super::super::GenerationResources<'_, R>,
    ) -> Result<Self, TransformError> {
        let original = input.into_declared();
        let buffered = original.clone();
        let mut prepared =
            Self::prepare_with_capabilities(buffered, endpoint, identities, state, resources)
                .await?;
        prepared.original_request = original;
        prepared.target_request.stream = Some(Some(false));
        Ok(prepared)
    }
}

impl ResponsesViaGemini {
    /// Prepare a buffered upstream result for later native stream synthesis,
    /// retaining the original client stream flag and all declared controls.
    pub async fn prepare_for_stream_synthesis<S: crate::capability::StateStore>(
        input: r::GenerateContentRequestBody,
        endpoint: Endpoint,
        identities: GenerationIdentity,
        state: &super::super::GenerationStateAccess<'_, S>,
        context: p::GeminiReplayContext,
    ) -> Result<Self, TransformError> {
        let original = input.into_declared();
        let mut buffered = original.clone();
        buffered.stream = Some(Some(false));
        let mut prepared =
            Self::prepare_with_state(buffered, endpoint, identities, state, context).await?;
        prepared.original_request = original;
        Ok(prepared)
    }
    /// Prepare a buffered upstream result for later native stream synthesis,
    /// retaining the original client stream flag and all declared controls.
    pub async fn prepare_for_stream_synthesis_with_capabilities<
        S: crate::capability::StateStore,
        R: crate::capability::ResourceAccess,
    >(
        input: r::GenerateContentRequestBody,
        endpoint: Endpoint,
        identities: GenerationIdentity,
        state: &super::super::GenerationStateAccess<'_, S>,
        resources: &super::super::GenerationResources<'_, R>,
        context: p::GeminiReplayContext,
    ) -> Result<Self, TransformError> {
        let original = input.into_declared();
        let mut buffered = original.clone();
        buffered.stream = Some(Some(false));
        let mut prepared = Self::prepare_with_capabilities(
            buffered, endpoint, identities, state, resources, context,
        )
        .await?;
        prepared.original_request = original;
        Ok(prepared)
    }
}

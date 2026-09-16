use super::*;

impl ClaudeViaResponses {
    /// Prepare a buffered upstream result for later native stream synthesis,
    /// retaining the original client stream flag and all declared controls.
    pub async fn prepare_for_stream_synthesis<S: crate::capability::StateStore>(
        input: c::GenerateContentRequestBody,

        endpoint: Endpoint,
        identities: GenerationIdentity,
        state: &super::super::GenerationStateAccess<'_, S>,
    ) -> Result<Self, TransformError> {
        let original = input.into_declared();
        let mut buffered = original.clone();
        buffered.stream = Some(false);
        let mut prepared = Self::prepare_with_state(buffered, endpoint, identities, state).await?;
        prepared.original_request = original;
        prepared.target_request.stream = Some(Some(false));
        Ok(prepared)
    }
}

impl ClaudeViaResponses {
    /// Prepare a buffered upstream result for later native stream synthesis,
    /// retaining the original client stream flag and all declared controls.
    pub async fn prepare_for_stream_synthesis_with_capabilities<
        S: crate::capability::StateStore,
        R: crate::capability::ResourceAccess,
    >(
        input: c::GenerateContentRequestBody,

        endpoint: Endpoint,
        identities: GenerationIdentity,
        state: &super::super::GenerationStateAccess<'_, S>,
        resources: &super::super::GenerationResources<'_, R>,
    ) -> Result<Self, TransformError> {
        let original = input.into_declared();
        let mut buffered = original.clone();
        buffered.stream = Some(false);
        let mut prepared =
            Self::prepare_with_capabilities(buffered, endpoint, identities, state, resources)
                .await?;
        prepared.original_request = original;
        prepared.target_request.stream = Some(Some(false));
        Ok(prepared)
    }
}

impl ResponsesViaClaude {
    /// Prepare a buffered upstream result for later native stream synthesis,
    /// retaining the original client stream flag and all declared controls.
    pub async fn prepare_for_stream_synthesis<S: crate::capability::StateStore>(
        input: r::GenerateContentRequestBody,

        endpoint: Endpoint,
        identities: GenerationIdentity,
        state: &super::super::GenerationStateAccess<'_, S>,
        context: p::ClaudeRequestContext,
    ) -> Result<Self, TransformError> {
        let original = input.into_declared();
        let mut buffered = original.clone();
        buffered.stream = Some(Some(false));
        let mut prepared =
            Self::prepare_with_state(buffered, endpoint, identities, state, context).await?;
        prepared.original_request = original;
        prepared.target_request.stream = Some(false);
        Ok(prepared)
    }
}

impl ResponsesViaClaude {
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
        context: p::ClaudeRequestContext,
    ) -> Result<Self, TransformError> {
        let original = input.into_declared();
        let mut buffered = original.clone();
        buffered.stream = Some(Some(false));
        let mut prepared = Self::prepare_with_capabilities(
            buffered, endpoint, identities, state, resources, context,
        )
        .await?;
        prepared.original_request = original;
        prepared.target_request.stream = Some(false);
        Ok(prepared)
    }
}

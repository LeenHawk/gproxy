use super::*;

impl ClaudeViaGemini {
    /// Prepare a buffered upstream result for later native stream synthesis,
    /// retaining the original client stream flag and all declared controls.
    pub async fn prepare_for_stream_synthesis<S: crate::capability::StateStore>(
        input: c::GenerateContentRequestBody,
        selected_model: impl Into<String>,
        endpoint: Endpoint,
        identities: GenerationIdentity,
        state: &super::super::GenerationStateAccess<'_, S>,
        context: p::ClaudeGeminiRequestContext,
    ) -> Result<Self, TransformError> {
        let original = input.into_declared();
        let mut buffered = original.clone();
        buffered.stream = Some(false);
        let mut prepared = Self::prepare_with_state(
            buffered,
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
}

impl ClaudeViaGemini {
    /// Prepare a buffered upstream result for later native stream synthesis,
    /// retaining the original client stream flag and all declared controls.
    pub async fn prepare_for_stream_synthesis_with_capabilities<
        S: crate::capability::StateStore,
        R: crate::capability::ResourceAccess,
    >(
        input: c::GenerateContentRequestBody,
        selected_model: impl Into<String>,
        endpoint: Endpoint,
        identities: GenerationIdentity,
        state: &super::super::GenerationStateAccess<'_, S>,
        resources: &super::super::GenerationResources<'_, R>,
        context: p::ClaudeGeminiRequestContext,
    ) -> Result<Self, TransformError> {
        let original = input.into_declared();
        let mut buffered = original.clone();
        buffered.stream = Some(false);
        let mut prepared = Self::prepare_with_capabilities(
            buffered,
            selected_model,
            endpoint,
            identities,
            state,
            resources,
            context,
        )
        .await?;
        prepared.original_request = original;
        Ok(prepared)
    }
}

impl GeminiViaClaude {
    /// Prepare a buffered upstream result for later native stream synthesis,
    /// retaining the original client stream flag and all declared controls.
    pub async fn prepare_for_stream_synthesis<S: crate::capability::StateStore>(
        input: g::GenerateContentRequestBody,
        selected_model: impl Into<String>,
        endpoint: Endpoint,
        identities: GenerationIdentity,
        state: &super::super::GenerationStateAccess<'_, S>,
        max_tokens: Option<i64>,
    ) -> Result<Self, TransformError> {
        let original = input.into_declared();
        let buffered = original.clone();
        let mut prepared = Self::prepare_with_state(
            buffered,
            selected_model,
            endpoint,
            identities,
            state,
            max_tokens,
        )
        .await?;
        prepared.original_request = original;
        prepared.target_request.stream = Some(false);
        Ok(prepared)
    }
}

impl GeminiViaClaude {
    /// Prepare a buffered upstream result for later native stream synthesis,
    /// retaining the original client stream flag and all declared controls.
    pub async fn prepare_for_stream_synthesis_with_capabilities<
        S: crate::capability::StateStore,
        R: crate::capability::ResourceAccess,
    >(
        input: g::GenerateContentRequestBody,
        selected_model: impl Into<String>,
        endpoint: Endpoint,
        identities: GenerationIdentity,
        state: &super::super::GenerationStateAccess<'_, S>,
        resources: &super::super::GenerationResources<'_, R>,
        max_tokens: Option<i64>,
    ) -> Result<Self, TransformError> {
        let original = input.into_declared();
        let buffered = original.clone();
        let mut prepared = Self::prepare_with_capabilities(
            buffered,
            selected_model,
            endpoint,
            identities,
            state,
            resources,
            max_tokens,
        )
        .await?;
        prepared.original_request = original;
        prepared.target_request.stream = Some(false);
        Ok(prepared)
    }
}

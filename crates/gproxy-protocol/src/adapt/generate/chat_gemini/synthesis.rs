use super::*;

impl ChatViaGemini {
    /// Prepare a buffered upstream result for later native stream synthesis,
    /// retaining the original client stream flag and all declared controls.
    pub async fn prepare_for_stream_synthesis<S: crate::capability::StateStore>(
        input: h::GenerateContentRequestBody,
        selected_model: impl Into<String>,
        endpoint: Endpoint,
        identities: GenerationIdentity,
        state: &super::super::GenerationStateAccess<'_, S>,
        function_names: &std::collections::BTreeMap<String, String>,
    ) -> Result<Self, TransformError> {
        let original = input.into_declared();
        let mut buffered = original.clone();
        buffered.stream = Some(Some(false));
        let mut prepared = Self::prepare_with_state(
            buffered,
            selected_model,
            endpoint,
            identities,
            state,
            function_names,
        )
        .await?;
        prepared.original_request = original;
        Ok(prepared)
    }
}

impl ChatViaGemini {
    /// Prepare a buffered upstream result for later native stream synthesis,
    /// retaining the original client stream flag and all declared controls.
    pub async fn prepare_for_stream_synthesis_with_capabilities<
        S: crate::capability::StateStore,
        R: crate::capability::ResourceAccess,
    >(
        input: h::GenerateContentRequestBody,
        selected_model: impl Into<String>,
        endpoint: Endpoint,
        identities: GenerationIdentity,
        state: &super::super::GenerationStateAccess<'_, S>,
        resources: &super::super::GenerationResources<'_, R>,
        function_names: &std::collections::BTreeMap<String, String>,
    ) -> Result<Self, TransformError> {
        let original = input.into_declared();
        let mut buffered = original.clone();
        buffered.stream = Some(Some(false));
        let mut prepared = Self::prepare_with_capabilities(
            buffered,
            selected_model,
            endpoint,
            identities,
            state,
            resources,
            function_names,
        )
        .await?;
        prepared.original_request = original;
        Ok(prepared)
    }
}

impl GeminiViaChat {
    /// Prepare a buffered upstream result for later native stream synthesis,
    /// retaining the original client stream flag and all declared controls.
    pub async fn prepare_for_stream_synthesis<S: crate::capability::StateStore>(
        input: g::GenerateContentRequestBody,
        selected_model: impl Into<String>,
        endpoint: Endpoint,
        identities: GenerationIdentity,
        state: &super::super::GenerationStateAccess<'_, S>,
    ) -> Result<Self, TransformError> {
        let original = input.into_declared();
        let buffered = original.clone();
        let mut prepared =
            Self::prepare_with_state(buffered, selected_model, endpoint, identities, state).await?;
        prepared.original_request = original;
        prepared.target_request.stream = Some(Some(false));
        Ok(prepared)
    }
}

impl GeminiViaChat {
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
        )
        .await?;
        prepared.original_request = original;
        prepared.target_request.stream = Some(Some(false));
        Ok(prepared)
    }
}

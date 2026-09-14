use super::*;
#[derive(Debug)]
pub struct GeminiViaResponsesFanout(Fanout<GeminiViaResponses>);
impl GeminiViaResponsesFanout {
    pub async fn prepare<S: StateStore>(
        input: g::GenerateContentRequestBody,
        target: FanoutTarget,
        state: &GenerationStateAccess<'_, S>,
        limits: CodecLimits,
    ) -> Result<Self, TransformError> {
        let mut input = input.into_declared();
        let id = validate(&target, gemini_count(&input)?)?;
        let original = encode(&input, limits)?;
        input
            .generation_config
            .as_mut()
            .expect("count set")
            .candidate_count = Some(1);
        let mut children = Vec::new();
        for identities in target.identities {
            children.push(
                GeminiViaResponses::prepare_with_state(
                    input.clone(),
                    target.model.clone(),
                    target.endpoint.clone(),
                    identities,
                    state,
                )
                .await?,
            );
        }
        Ok(Self(Fanout {
            children,
            endpoint: target.endpoint,
            group_id: id,
            original,
            options: target.options,
        }))
    }
    /// Resources are read once before cloning the single-candidate request.
    pub async fn prepare_with_capabilities<S: StateStore, R: ResourceAccess>(
        input: g::GenerateContentRequestBody,
        target: FanoutTarget,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
        limits: CodecLimits,
    ) -> Result<Self, TransformError> {
        let input = input.into_declared();
        validate(&target, gemini_count(&input)?)?;
        let original = encode(&input, limits)?;
        let mut prepared =
            Self::prepare(resources.gemini(input).await?, target, state, limits).await?;
        prepared.0.original = original;
        Ok(prepared)
    }
    pub fn response_id(&self) -> &str {
        &self.0.group_id
    }
    pub fn children(&self) -> &[GeminiViaResponses] {
        &self.0.children
    }
    /// Start a fresh journal and send each child at most once.
    pub async fn invoke<U: Upstream, S: StateStore>(
        &mut self,
        upstream: &U,
        target: &U::Target,
        limits: CodecLimits,
        state: &GenerationStateAccess<'_, S>,
        progress: &mut FanoutProgress<r::GenerateContentResponseBody>,
        facts: impl FnMut(
            usize,
            &r::GenerateContentResponseBody,
        ) -> Result<
            crate::transform::generate::gemini_responses::GeminiReplayContext,
            TransformError,
        >,
    ) -> Result<Converted<g::GenerateContentResponseBody>, TransformError> {
        self.0
            .run((upstream, target), limits, state, progress, facts, false)
            .await
    }
    /// Reload the exact reserved journal; only never-started children may send.
    pub async fn resume<U: Upstream, S: StateStore>(
        &mut self,
        upstream: &U,
        target: &U::Target,
        limits: CodecLimits,
        state: &GenerationStateAccess<'_, S>,
        progress: &mut FanoutProgress<r::GenerateContentResponseBody>,
        facts: impl FnMut(
            usize,
            &r::GenerateContentResponseBody,
        ) -> Result<
            crate::transform::generate::gemini_responses::GeminiReplayContext,
            TransformError,
        >,
    ) -> Result<Converted<g::GenerateContentResponseBody>, TransformError> {
        self.0
            .run((upstream, target), limits, state, progress, facts, true)
            .await
    }
}

use super::*;

#[derive(Debug)]
pub struct GeminiViaResponsesFanout(Fanout<GeminiViaResponses>);

impl GeminiViaResponsesFanout {
    pub async fn prepare<S: StateStore>(
        input: g::GenerateContentRequestBody,
        target: FanoutTarget,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<Self, TransformError> {
        let mut input = input.into_declared();
        let id = group_id(&target.options)?;
        let count = input
            .generation_config
            .as_ref()
            .and_then(|v| v.candidate_count)
            .unwrap_or(1);
        input
            .generation_config
            .get_or_insert_with(|| g::GenerationConfig::builder().build())
            .candidate_count = Some(1);
        let mut children = Vec::new();
        for index in 0..count {
            let identities = target.options.child_identity(index, crate::Dialect::OpenAi);
            children.push(
                GeminiViaResponses::prepare_with_state(
                    input.clone(),
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
            started: false,
        }))
    }
    /// Resources are read once before cloning the single-candidate request.
    pub async fn prepare_with_capabilities<S: StateStore, R: ResourceAccess>(
        input: g::GenerateContentRequestBody,
        target: FanoutTarget,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
    ) -> Result<Self, TransformError> {
        let input = input.into_declared();
        Self::prepare(resources.gemini(input).await?, target, state).await
    }
    pub fn response_id(&self) -> &str {
        &self.0.group_id
    }
    pub fn children(&self) -> &[GeminiViaResponses] {
        &self.0.children
    }
    /// Sends each child at most once and aggregates their results in order.
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
            .run((upstream, target), limits, state, progress, facts)
            .await
    }
}

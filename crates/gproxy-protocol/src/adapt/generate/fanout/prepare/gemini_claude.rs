use super::*;

#[derive(Debug)]
pub struct GeminiViaClaudeFanout(Fanout<GeminiViaClaude>);

impl GeminiViaClaudeFanout {
    pub async fn prepare<S: StateStore>(
        input: g::GenerateContentRequestBody,
        target: FanoutTarget,
        state: &GenerationStateAccess<'_, S>,
        max_tokens: Option<i64>,
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
            let identities = target.options.child_identity(index, crate::Dialect::Claude);
            children.push(
                GeminiViaClaude::prepare_with_state(
                    input.clone(),
                    target.endpoint.clone(),
                    identities,
                    state,
                    max_tokens,
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
        max_tokens: Option<i64>,
    ) -> Result<Self, TransformError> {
        let input = input.into_declared();
        Self::prepare(resources.gemini(input).await?, target, state, max_tokens).await
    }
    pub fn response_id(&self) -> &str {
        &self.0.group_id
    }
    pub fn children(&self) -> &[GeminiViaClaude] {
        &self.0.children
    }
    /// Sends each child at most once and aggregates their results in order.
    pub async fn invoke<U: Upstream, S: StateStore>(
        &mut self,
        upstream: &U,
        target: &U::Target,
        limits: CodecLimits,
        state: &GenerationStateAccess<'_, S>,
        progress: &mut FanoutProgress<c::GenerateContentResponseBody>,
        facts: impl FnMut(
            usize,
            &c::GenerateContentResponseBody,
        ) -> Result<
            crate::transform::generate::claude_gemini::ClaudeGeminiUsageFacts,
            TransformError,
        >,
    ) -> Result<Converted<g::GenerateContentResponseBody>, TransformError> {
        self.0
            .run((upstream, target), limits, state, progress, facts)
            .await
    }
}

use super::*;
#[derive(Debug)]
pub struct ChatViaResponsesFanout(Fanout<ChatViaResponses>);
impl ChatViaResponsesFanout {
    pub async fn prepare<S: StateStore>(
        input: h::GenerateContentRequestBody,
        target: FanoutTarget,
        state: &GenerationStateAccess<'_, S>,
        limits: CodecLimits,
    ) -> Result<Self, TransformError> {
        let mut input = input.into_declared();
        let id = group_id(&target.options)?;
        let count = input.n.flatten().unwrap_or(1);
        let original = encode(&input, limits)?;
        input.n = Some(Some(1));
        let mut children = Vec::new();
        for index in 0..count {
            let identities = target.options.child_identity(index, crate::Dialect::OpenAi);
            children.push(
                ChatViaResponses::prepare_with_state(
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
            original,
            options: target.options,
        }))
    }
    /// Resources are read once before cloning the single-candidate request.
    pub async fn prepare_with_capabilities<S: StateStore, R: ResourceAccess>(
        input: h::GenerateContentRequestBody,
        target: FanoutTarget,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
        limits: CodecLimits,
    ) -> Result<Self, TransformError> {
        let input = input.into_declared();
        let original = encode(&input, limits)?;
        let mut prepared =
            Self::prepare(resources.chat(input).await?, target, state, limits).await?;
        prepared.0.original = original;
        Ok(prepared)
    }
    pub fn response_id(&self) -> &str {
        &self.0.group_id
    }
    pub fn children(&self) -> &[ChatViaResponses] {
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
        facts: impl FnMut(usize, &r::GenerateContentResponseBody) -> Result<(), TransformError>,
    ) -> Result<Converted<h::GenerateContentResponseBody>, TransformError> {
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
        facts: impl FnMut(usize, &r::GenerateContentResponseBody) -> Result<(), TransformError>,
    ) -> Result<Converted<h::GenerateContentResponseBody>, TransformError> {
        self.0
            .run((upstream, target), limits, state, progress, facts, true)
            .await
    }
}

use super::*;
/// Concrete child advancement only; wire events remain the selected pair's types.
pub(super) trait ChildNext<B: FanoutBridge, S: StateStore> {
    async fn next(
        &mut self,
        child: &mut StreamInvocation<B>,
        index: usize,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<Option<StreamChunk<B::ClientEvent>>, TransformError>;
}
pub(super) struct Plain;
impl<B: FanoutBridge, S: StateStore> ChildNext<B, S> for Plain {
    async fn next(
        &mut self,
        child: &mut StreamInvocation<B>,
        _: usize,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<Option<StreamChunk<B::ClientEvent>>, TransformError> {
        child.next(state).await
    }
}

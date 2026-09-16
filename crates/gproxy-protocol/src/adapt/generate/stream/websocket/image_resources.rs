use super::*;
use crate::{
    adapt::generate::{
        GenerationResources,
        image_resources::{ImageStreamProgress, ResourceSend, ResourceSync},
    },
    capability::ResourceAccess,
    transform::generate::gemini_responses::stream::ResponsesToGeminiStream,
    wire::gemini as g,
};

impl GenerationWsTurn<'_, ResponsesToGeminiStream> {
    /// Perform actual URI publications while retaining the native WS turn and
    /// pending client events across cancellation of this individual future.
    pub async fn next_with_image_resources<S, R>(
        &mut self,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
        progress: &mut ImageStreamProgress<R::PublishedHandle>,
    ) -> Result<Option<StreamChunk<g::GenerateContentResponseBody>>, TransformError>
    where
        R: ResourceAccess + ResourceSync,
        R::Scope: ResourceSync,
        R::PublishedHandle: ResourceSend,
        S: StateStore + ResourceSync,
        S::Scope: ResourceSync,
    {
        let Some(turn) = &mut self.turn else {
            return Err(super::super::invalid("WebSocket generation turn failed"));
        };
        progress.bind(self.invocation.selected.identities.response.namespace())?;
        self.invocation.image_resources_required = true;
        let enabled = crate::adapt::generate::image_resources::wants_uri(&self.invocation.original);
        let mut mapping = super::super::resource_map::PublishImages {
            resources,
            progress,
            expires_at: state.expires_at,
            enabled,
        };
        let result = self
            .invocation
            .next_mapped(state, Some(turn), Some(&mut mapping))
            .await;
        if result.is_err() && self.invocation.failed {
            self.failure = turn.failure_event().cloned();
            self.websocket_failure = turn.websocket_failure().cloned();
            self.turn.take();
        }
        result
    }
}

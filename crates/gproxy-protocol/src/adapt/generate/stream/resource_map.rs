//! Async resource work over the two concrete Gemini/Responses event types.

use super::{
    StreamInvocation,
    bridge::StreamBridge,
    event::Collected,
    invoke::{ClientFull, NativeFull},
};
use crate::{
    adapt::generate::{
        GenerationResources, GenerationStateAccess,
        image_resources::{ImageStreamProgress, ResourceSend, ResourceSync},
    },
    capability::{ResourceAccess, StateStore},
    transform::{
        TransformError,
        generate::gemini_responses::stream::{GeminiToResponsesStream, ResponsesToGeminiStream},
        identity::IdentityFlow,
    },
    wire::{gemini as g, openai::responses::stream::StreamEvent},
};

type Work<'a, T> = crate::capability::CapabilityFuture<'a, Result<T, TransformError>>;

#[cfg(not(target_arch = "wasm32"))]
pub(super) type ExternalSource<'a, E> =
    dyn futures_core::Stream<Item = Result<E, TransformError>> + Unpin + Send + 'a;

#[cfg(target_arch = "wasm32")]
pub(super) type ExternalSource<'a, E> =
    dyn futures_core::Stream<Item = Result<E, TransformError>> + Unpin + 'a;

pub(super) trait ResourceMapping<B: StreamBridge, S: StateStore>: ResourceSend {
    fn revision(&self) -> u64;
    fn failed(&self) -> bool;
    fn begin_step(&mut self) -> Result<u64, TransformError>;
    fn source<'a>(&'a mut self, input: &'a B::NativeEvent) -> Work<'a, B::NativeEvent>;
    fn client<'a>(&'a mut self, input: &'a [B::ClientEvent]) -> Work<'a, Vec<B::ClientEvent>>;
    fn save<'a>(
        &'a mut self,
        native: &'a Collected<NativeFull<B>>,
        client: &'a ClientFull<B>,
        flow: &'a IdentityFlow,
        state: &'a GenerationStateAccess<'_, S>,
    ) -> Work<'a, ()>;
}

pub(super) struct ReadImages<'a, 'b, R: ResourceAccess> {
    resources: &'a GenerationResources<'b, R>,
    progress: &'a mut ImageStreamProgress<R::PublishedHandle>,
}

impl<R: ResourceAccess + ResourceSync, S: StateStore + ResourceSync>
    ResourceMapping<GeminiToResponsesStream, S> for ReadImages<'_, '_, R>
where
    R::Scope: ResourceSync,
    R::PublishedHandle: ResourceSend,
    S::Scope: ResourceSync,
{
    fn revision(&self) -> u64 {
        self.progress.revision
    }
    fn failed(&self) -> bool {
        self.progress.failed()
    }
    fn begin_step(&mut self) -> Result<u64, TransformError> {
        self.progress.committed(())?;
        Ok(self.progress.revision)
    }
    fn source<'a>(
        &'a mut self,
        input: &'a g::GenerateContentResponseBody,
    ) -> Work<'a, g::GenerateContentResponseBody> {
        Box::pin(async move {
            let value = self.progress.source(input, self.resources).await?;
            self.progress.committed(value)
        })
    }
    fn client<'a>(&'a mut self, input: &'a [StreamEvent]) -> Work<'a, Vec<StreamEvent>> {
        Box::pin(async move { self.progress.committed(input.to_vec()) })
    }
    fn save<'a>(
        &'a mut self,
        _: &'a Collected<g::GenerateContentResponseBody>,
        client: &'a ClientFull<GeminiToResponsesStream>,
        flow: &'a IdentityFlow,
        state: &'a GenerationStateAccess<'_, S>,
    ) -> Work<'a, ()> {
        Box::pin(async move {
            let images: Vec<_> = self.progress.reads.images().cloned().collect();
            state
                .save_file_image_proofs(&images, client, flow, &mut self.progress.proofs)
                .await?;
            self.progress.committed(())
        })
    }
}

pub(super) struct PublishImages<'a, 'b, R: ResourceAccess> {
    pub(super) resources: &'a GenerationResources<'b, R>,
    pub(super) progress: &'a mut ImageStreamProgress<R::PublishedHandle>,
    pub(super) expires_at: std::time::SystemTime,
    pub(super) enabled: bool,
}

impl<R: ResourceAccess + ResourceSync, S: StateStore + ResourceSync>
    ResourceMapping<ResponsesToGeminiStream, S> for PublishImages<'_, '_, R>
where
    R::Scope: ResourceSync,
    R::PublishedHandle: ResourceSend,
    S::Scope: ResourceSync,
{
    fn revision(&self) -> u64 {
        self.progress.revision
    }
    fn failed(&self) -> bool {
        self.progress.failed()
    }
    fn begin_step(&mut self) -> Result<u64, TransformError> {
        self.progress.committed(())?;
        Ok(self.progress.revision)
    }
    fn source<'a>(&'a mut self, input: &'a StreamEvent) -> Work<'a, StreamEvent> {
        Box::pin(async move { self.progress.committed(input.clone()) })
    }
    fn client<'a>(
        &'a mut self,
        input: &'a [g::GenerateContentResponseBody],
    ) -> Work<'a, Vec<g::GenerateContentResponseBody>> {
        Box::pin(async move {
            let value = if self.enabled {
                self.progress
                    .client(input, self.expires_at, self.resources)
                    .await?
            } else {
                input.to_vec()
            };
            self.progress.committed(value)
        })
    }
    fn save<'a>(
        &'a mut self,
        _: &'a Collected<NativeFull<ResponsesToGeminiStream>>,
        _: &'a g::GenerateContentResponseBody,
        _: &'a IdentityFlow,
        _: &'a GenerationStateAccess<'_, S>,
    ) -> Work<'a, ()> {
        Box::pin(async move { self.progress.committed(()) })
    }
}

impl StreamInvocation<GeminiToResponsesStream> {
    pub async fn next_with_image_resources<S, R>(
        &mut self,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
        progress: &mut ImageStreamProgress<R::PublishedHandle>,
    ) -> Result<Option<super::StreamChunk<StreamEvent>>, TransformError>
    where
        R: ResourceAccess + ResourceSync,
        R::Scope: ResourceSync,
        R::PublishedHandle: ResourceSend,
        S: StateStore + ResourceSync,
        S::Scope: ResourceSync,
    {
        progress.bind(self.selected.identities.response.namespace())?;
        self.image_resources_required = true;
        self.next_mapped(
            state,
            None,
            Some(&mut ReadImages {
                resources,
                progress,
            }),
        )
        .await
    }
}

impl StreamInvocation<ResponsesToGeminiStream> {
    pub async fn next_with_image_resources<S, R>(
        &mut self,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
        progress: &mut ImageStreamProgress<R::PublishedHandle>,
    ) -> Result<Option<super::StreamChunk<g::GenerateContentResponseBody>>, TransformError>
    where
        R: ResourceAccess + ResourceSync,
        R::Scope: ResourceSync,
        R::PublishedHandle: ResourceSend,
        S: StateStore + ResourceSync,
        S::Scope: ResourceSync,
    {
        progress.bind(self.selected.identities.response.namespace())?;
        self.image_resources_required = true;
        let enabled = crate::adapt::generate::image_resources::wants_uri(&self.original);
        let mut mapping = PublishImages {
            resources,
            progress,
            expires_at: state.expires_at,
            enabled,
        };
        if self.websocket_terminal {
            self.next_mapped(
                state,
                Some(&mut futures_util::stream::poll_fn(|_| {
                    std::task::Poll::Ready(None)
                })),
                Some(&mut mapping),
            )
            .await
        } else {
            self.next_mapped(state, None, Some(&mut mapping)).await
        }
    }
}

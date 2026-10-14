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

struct Images<'a, 'r, R: ResourceAccess> {
    resources: &'a GenerationResources<'r, R>,
    progress: &'a mut [ImageStreamProgress<R::PublishedHandle>],
}

impl<R: ResourceAccess + ResourceSync, S: StateStore + ResourceSync>
    super::driver::ChildNext<ResponsesToGeminiStream, S> for Images<'_, '_, R>
where
    R::Scope: ResourceSync,
    R::PublishedHandle: ResourceSend,
    S::Scope: ResourceSync,
{
    async fn next(
        &mut self,
        child: &mut StreamInvocation<ResponsesToGeminiStream>,
        index: usize,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<Option<StreamChunk<g::GenerateContentResponseBody>>, TransformError> {
        let mut used_bytes = 0u64;
        let mut used_count = 0usize;
        for (other, progress) in self.progress.iter().enumerate() {
            if other == index {
                continue;
            }
            let (count, bytes) = progress.publication_budget()?;
            used_bytes = used_bytes
                .checked_add(bytes)
                .ok_or_else(|| limit("fanout publication byte overflow"))?;
            used_count = used_count
                .checked_add(count)
                .ok_or_else(|| limit("fanout publication count overflow"))?;
        }
        let mut limits = self.resources.limits;
        limits.max_body_bytes = limits
            .max_body_bytes
            .checked_sub(used_bytes)
            .ok_or_else(|| limit("fanout publication byte budget exceeded"))?;
        let max_references = self
            .resources
            .max_references
            .checked_sub(used_count)
            .ok_or_else(|| limit("fanout publication count budget exceeded"))?;
        let resources = GenerationResources {
            access: self.resources.access,
            scope: self.resources.scope,
            limits,
            max_references,
            now: self.resources.now,
            target: self.resources.target,
        };
        child
            .next_with_image_resources(state, &resources, &mut self.progress[index])
            .await
    }
}

impl FanoutStream<ResponsesToGeminiStream> {
    /// Keep one progress entry per child. Publication operation IDs and receipts
    /// survive cancellation, and resource budgets apply to the whole group.
    pub async fn next_with_image_resources<U: crate::capability::Upstream, S, R>(
        &mut self,
        upstream: &U,
        target: &U::Target,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
        progress: &mut [ImageStreamProgress<R::PublishedHandle>],
    ) -> Result<Option<StreamChunk<g::GenerateContentResponseBody>>, TransformError>
    where
        R: ResourceAccess + ResourceSync,
        R::Scope: ResourceSync,
        R::PublishedHandle: ResourceSend,
        S: StateStore + ResourceSync,
        S::Scope: ResourceSync,
    {
        if progress.len() != self.children.len() {
            return Err(TransformError::shape(
                "fanout.image_progress",
                "one retained image progress entry per child required",
            ));
        }
        for (child, image_progress) in self.children.iter().zip(progress.iter_mut()) {
            if image_progress.revision != child.resource_revision {
                return Err(conflict(
                    "fanout image progress changed for an active or completed child",
                ));
            }
            image_progress.bind(child.selected.identities.response.namespace())?;
        }
        self.next_using(
            upstream,
            target,
            state,
            &mut Images {
                resources,
                progress,
            },
        )
        .await
    }
    pub async fn collect_with_image_resources<U: crate::capability::Upstream, S, R>(
        &mut self,
        upstream: &U,
        target: &U::Target,
        state: &GenerationStateAccess<'_, S>,
        resources: &GenerationResources<'_, R>,
        progress: &mut [ImageStreamProgress<R::PublishedHandle>],
    ) -> Result<crate::transform::Converted<g::GenerateContentResponseBody>, TransformError>
    where
        R: ResourceAccess + ResourceSync,
        R::Scope: ResourceSync,
        R::PublishedHandle: ResourceSend,
        S: StateStore + ResourceSync,
        S::Scope: ResourceSync,
    {
        while self
            .next_with_image_resources(upstream, target, state, resources, progress)
            .await?
            .is_some()
        {}
        Ok(crate::transform::Converted {
            value: self
                .client_result()
                .ok_or_else(|| invalid("aggregate not acknowledged"))?
                .clone(),
            report: self.report.clone(),
        })
    }
}

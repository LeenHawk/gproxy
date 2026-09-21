use super::*;
use crate::{capability::ResourceAccess, transform::identity::IdNamespace};

/// Resource progress for a single incremental invocation. Native events remain
/// in the invocation while these operations await; receipts and proofs survive
/// cancellation of an individual `next_with_image_resources` future.
pub struct ImageStreamProgress<H> {
    namespace: Option<IdNamespace>,
    pub(crate) revision: u64,
    source_index: u64,
    client_index: u64,
    native_parts: usize,
    source: BTreeMap<u64, GeminiImageReads>,
    client: BTreeMap<u64, GeminiImagePublications<H>>,
    pub(crate) reads: GeminiImageReads,
    pub(crate) proofs: GenerationProgress<()>,
}

impl<H> Default for ImageStreamProgress<H> {
    fn default() -> Self {
        Self {
            namespace: None,
            revision: 0,
            source_index: 0,
            client_index: 0,
            native_parts: 0,
            source: BTreeMap::new(),
            client: BTreeMap::new(),
            reads: Default::default(),
            proofs: Default::default(),
        }
    }
}

impl<H> ImageStreamProgress<H> {
    pub(crate) fn failed(&self) -> bool {
        self.source.values().any(|event| event.failed)
            || self.client.values().any(|event| event.failed)
    }
    pub(crate) fn committed<T>(&mut self, value: T) -> Result<T, TransformError> {
        self.revision = self.revision.checked_add(1).ok_or_else(limit)?;
        Ok(value)
    }
    pub(crate) fn bind(&mut self, namespace: IdNamespace) -> Result<(), TransformError> {
        if self.namespace.is_some_and(|old| old != namespace) {
            return Err(conflict(
                "image stream progress belongs to another invocation",
            ));
        }
        self.namespace = Some(namespace);
        Ok(())
    }
    /// Includes uncertain operations whose body must not be submitted again.
    pub fn publication_budget(&self) -> Result<(usize, u64), TransformError> {
        let mut count = 0usize;
        let mut bytes = 0u64;
        for image in self
            .client
            .values()
            .flat_map(|event| event.publications.values())
        {
            let padding = if image.source.data.ends_with("==") {
                2
            } else {
                usize::from(image.source.data.ends_with('='))
            };
            let size = (image.source.data.len() / 4 * 3).saturating_sub(padding) as u64;
            count = count.checked_add(1).ok_or_else(limit)?;
            bytes = bytes.checked_add(size).ok_or_else(limit)?;
        }
        Ok((count, bytes))
    }
    pub fn publication_receipts(&self) -> impl Iterator<Item = &PublishedResource<H>> {
        self.client.values().flat_map(|event| event.receipts())
    }
    pub fn publication_ids(&self) -> impl Iterator<Item = &str> {
        self.client.values().flat_map(|event| event.operation_ids())
    }
    pub(crate) async fn source<R: ResourceAccess<PublishedHandle = H>>(
        &mut self,
        input: &g::GenerateContentResponseBody,
        resources: &super::super::GenerationResources<'_, R>,
    ) -> Result<g::GenerateContentResponseBody, TransformError> {
        let parts: usize = input
            .candidates
            .iter()
            .flatten()
            .flat_map(|candidate| candidate.content.iter())
            .map(|content| content.parts.as_ref().map_or(0, Vec::len))
            .sum();
        let has_files = input
            .candidates
            .iter()
            .flatten()
            .flat_map(|candidate| candidate.content.iter())
            .flat_map(|content| content.parts.iter().flatten())
            .any(|part| part.file_data.is_some());
        let value = if has_files {
            let used_bytes: u64 = self
                .source
                .iter()
                .filter(|(index, _)| **index != self.source_index)
                .map(|(_, event)| event.bytes)
                .sum();
            let used_reads: usize = self
                .source
                .iter()
                .filter(|(index, _)| **index != self.source_index)
                .map(|(_, event)| event.attempted)
                .sum();
            let mut limits = resources.limits;
            limits.max_body_bytes = limits.max_body_bytes.saturating_sub(used_bytes);
            let bound = super::super::GenerationResources {
                access: resources.access,
                scope: resources.scope,
                limits,
                max_references: resources.max_references.saturating_sub(used_reads),
                now: resources.now,
            };
            let event = self.source.entry(self.source_index).or_default();
            let value = event.materialize(input, &bound).await?;
            for image in event.images() {
                let mut image = image.clone();
                image.part = image
                    .part
                    .checked_add(self.native_parts)
                    .ok_or_else(limit)?;
                self.reads
                    .images
                    .insert((image.candidate, image.part), image);
            }
            value
        } else {
            input.clone()
        };
        self.native_parts = self.native_parts.checked_add(parts).ok_or_else(limit)?;
        self.source_index = self.source_index.checked_add(1).ok_or_else(limit)?;
        Ok(value)
    }
    pub(crate) async fn client<R: ResourceAccess<PublishedHandle = H>>(
        &mut self,
        input: &[g::GenerateContentResponseBody],
        expires_at: std::time::SystemTime,
        resources: &super::super::GenerationResources<'_, R>,
    ) -> Result<Vec<g::GenerateContentResponseBody>, TransformError> {
        let mut output = Vec::with_capacity(input.len());
        // Advance only after the complete converted batch succeeds. An awaited
        // publication may be canceled after earlier events in this batch finish.
        for (offset, event) in input.iter().enumerate() {
            let index = self
                .client_index
                .checked_add(offset as u64)
                .ok_or_else(limit)?;
            let has_images = event
                .candidates
                .iter()
                .flatten()
                .flat_map(|candidate| candidate.content.iter())
                .flat_map(|content| content.parts.iter().flatten())
                .any(|part| part.inline_data.is_some() || part.file_data.is_some());
            if !has_images {
                output.push(event.clone());
                continue;
            }
            let used_bytes: u64 = self
                .client
                .iter()
                .filter(|(old, _)| **old != index)
                .flat_map(|(_, event)| event.publications.values())
                .map(|image| {
                    let padding = if image.source.data.ends_with("==") {
                        2
                    } else {
                        usize::from(image.source.data.ends_with('='))
                    };
                    (image.source.data.len() / 4 * 3).saturating_sub(padding) as u64
                })
                .sum();
            let used_count: usize = self
                .client
                .iter()
                .filter(|(old, _)| **old != index)
                .map(|(_, event)| event.publications.len())
                .sum();
            let mut limits = resources.limits;
            limits.max_body_bytes = limits.max_body_bytes.saturating_sub(used_bytes);
            let bound = super::super::GenerationResources {
                access: resources.access,
                scope: resources.scope,
                limits,
                max_references: resources.max_references.saturating_sub(used_count),
                now: resources.now,
            };
            let value = self
                .client
                .entry(index)
                .or_default()
                .publish_event(
                    event,
                    self.namespace
                        .ok_or_else(|| missing("unbound image stream progress"))?,
                    index,
                    expires_at,
                    &bound,
                )
                .await?;
            output.push(value);
        }
        self.client_index = self
            .client_index
            .checked_add(input.len() as u64)
            .ok_or_else(limit)?;
        Ok(output)
    }
}

use super::*;
use crate::{
    HttpBody,
    capability::{ResourceAccess, ResourceReference},
    codec,
    transform::images::inspect_image,
    wire::DeclaredFields,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use futures_util::StreamExt;

impl GeminiImageReads {
    /// Returns a separate inline conversion view; original fileData and its
    /// signature stay in retained progress for exact scoped replay.
    pub async fn materialize<R: ResourceAccess>(
        &mut self,
        input: &g::GenerateContentResponseBody,
        resources: &super::super::GenerationResources<'_, R>,
    ) -> Result<g::GenerateContentResponseBody, TransformError> {
        if self.failed {
            return Err(invalid("image read progress failed"));
        }
        let result = self.materialize_inner(input, resources).await;
        if result.as_ref().err().is_some_and(|error| {
            matches!(
                error.kind(),
                TransformErrorKind::InvalidInput
                    | TransformErrorKind::InvalidResult
                    | TransformErrorKind::Unsupported
                    | TransformErrorKind::Limit
            )
        }) {
            self.pending = None;
            self.failed = true;
        }
        result
    }
    async fn materialize_inner<R: ResourceAccess>(
        &mut self,
        input: &g::GenerateContentResponseBody,
        resources: &super::super::GenerationResources<'_, R>,
    ) -> Result<g::GenerateContentResponseBody, TransformError> {
        let input = input.clone().into_declared();
        codec::encode_json(&input, resources.limits).map_err(codec_error)?;
        if let Some(original) = &self.original {
            if original != &input {
                return Err(conflict("native image response changed during recovery"));
            }
        } else {
            self.original = Some(input.clone());
        }
        let mut output = input;
        for (candidate_index, candidate) in output.candidates.iter_mut().flatten().enumerate() {
            for (part_index, part) in candidate
                .content
                .iter_mut()
                .flat_map(|content| content.parts.iter_mut().flatten())
                .enumerate()
            {
                let Some(file) = &part.file_data else {
                    continue;
                };
                if part.inline_data.is_some()
                    || part.text.is_some()
                    || part.thought == Some(true)
                    || part.function_call.is_some()
                    || part.function_response.is_some()
                    || part.executable_code.is_some()
                    || part.code_execution_result.is_some()
                    || part.tool_call.is_some()
                    || part.tool_response.is_some()
                    || part.video_metadata.is_some()
                    || part.media_processing.is_some()
                    || part.audio_transcription.is_some()
                    || part.speech_metadata.is_some()
                    || part.part_metadata.is_some()
                    || part.media_resolution.is_some()
                    || file.file_uri.trim().is_empty()
                {
                    return Err(invalid(
                        "generated file image must be an unambiguous standalone Part",
                    ));
                }
                let key = (candidate_index, part_index);
                if !self.images.contains_key(&key) {
                    self.read_image(key, file, resources).await?;
                    let pending = self.pending.take().expect("completed retained read");
                    let actual = inspect_image(&pending.buffer)?.mime();
                    if pending
                        .metadata
                        .mime
                        .as_deref()
                        .is_some_and(|mime| mime != actual)
                        || file.mime_type.as_deref().is_some_and(|mime| mime != actual)
                    {
                        return Err(invalid("generated image MIME contradicts actual container"));
                    }
                    self.images.insert(
                        key,
                        MaterializedImage {
                            candidate: candidate_index,
                            part: part_index,
                            original: part.clone(),
                            resource_expiry: pending.metadata.expires_at,
                            materialized: g::Blob::builder(
                                actual.to_owned(),
                                STANDARD.encode(pending.buffer),
                            )
                            .build(),
                        },
                    );
                }
                let image = self.images.get(&key).expect("materialized image retained");
                if &image.original != part {
                    return Err(conflict("native image Part changed"));
                }
                part.file_data = None;
                // The signature stays: the client carries it back with these
                // bytes, which replay inline in place of the fileData URI.
                part.inline_data = Some(image.materialized.clone());
            }
        }
        codec::encode_json(&output, resources.limits).map_err(codec_error)?;
        Ok(output)
    }
    async fn read_image<R: ResourceAccess>(
        &mut self,
        key: (usize, usize),
        file: &g::FileData,
        resources: &super::super::GenerationResources<'_, R>,
    ) -> Result<(), TransformError> {
        let reference = ResourceReference::Url(file.file_uri.clone());
        let total = resources
            .limits
            .max_body_bytes
            .min(resources.access.limits().read_bytes);
        if let Some(pending) = &self.pending {
            if pending.key != key || pending.reference != reference {
                return Err(conflict("pending image read changed"));
            }
        } else {
            if self.attempted >= resources.max_references || self.bytes >= total {
                return Err(limit());
            }
            self.attempted += 1;
            let read = resources.access.read(resources.scope, &reference).await?;
            self.pending = Some(PendingImageRead {
                key,
                reference,
                metadata: read.metadata,
                body: read.body,
                buffer: Vec::new(),
                limit: total
                    .saturating_sub(self.bytes)
                    .min(resources.limits.max_buffer_bytes)
                    .min(resources.limits.max_part_bytes),
            });
        }
        let pending = self.pending.as_mut().expect("retained resource body");
        if pending
            .metadata
            .expires_at
            .is_some_and(|expiry| expiry <= resources.now)
        {
            return Err(missing("generated image resource expired"));
        }
        let available = total
            .saturating_sub(self.bytes)
            .saturating_add(pending.buffer.len() as u64);
        pending.limit = pending
            .limit
            .min(available)
            .min(resources.limits.max_buffer_bytes)
            .min(resources.limits.max_part_bytes);
        if pending.metadata.length.is_some_and(|n| n > pending.limit) || self.bytes > total {
            return Err(limit());
        }
        let mut chunks_since_yield = 0usize;
        loop {
            // Body, accumulated bytes and accounting are owned by progress,
            // rather than by the cancellable future calling this method.
            let next = match &mut self.pending.as_mut().expect("retained body").body {
                HttpBody::Bytes(bytes) => {
                    if bytes.is_empty() {
                        None
                    } else {
                        Some(Ok(std::mem::take(bytes)))
                    }
                }
                HttpBody::Stream(stream) => stream.next().await,
            };
            let Some(chunk) = next else {
                break;
            };
            let chunk = chunk.map_err(|error| {
                TransformError::with_source(
                    TransformErrorKind::Host,
                    "generation.image_resources.read",
                    error.to_string(),
                    error,
                )
            })?;
            self.bytes = self
                .bytes
                .checked_add(chunk.len() as u64)
                .ok_or_else(limit)?;
            let pending = self.pending.as_mut().expect("retained body");
            if self.bytes > total
                || (pending.buffer.len() as u64).saturating_add(chunk.len() as u64) > pending.limit
            {
                return Err(limit());
            }
            pending.buffer.extend_from_slice(&chunk);
            chunks_since_yield += 1;
            if chunks_since_yield == 1024 {
                chunks_since_yield = 0;
                let mut yielded = false;
                std::future::poll_fn(|cx| {
                    if yielded {
                        std::task::Poll::Ready(())
                    } else {
                        yielded = true;
                        cx.waker().wake_by_ref();
                        std::task::Poll::Pending
                    }
                })
                .await;
            }
        }
        let pending = self.pending.as_ref().expect("complete retained body");
        if pending
            .metadata
            .length
            .is_some_and(|n| n != pending.buffer.len() as u64)
        {
            return Err(invalid(
                "generated image length contradicts resource metadata",
            ));
        }
        Ok(())
    }
}

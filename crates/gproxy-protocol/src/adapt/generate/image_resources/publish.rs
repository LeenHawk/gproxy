use super::*;
use crate::{
    HttpBody,
    capability::{
        PublicationKind, PublicationStatus, ResourceAccess, ResourceMetadata, ResourceReference,
    },
    transform::{identity::IdNamespace, images::decode_image},
    wire::DeclaredFields,
};

impl<H> GeminiImagePublications<H> {
    /// Publish validated image bytes as actual host URLs. A lost publish result
    /// is recovered by status only; Missing/Pending/Expired never replay a body.
    pub async fn publish<R: ResourceAccess<PublishedHandle = H>>(
        &mut self,
        input: &g::GenerateContentResponseBody,
        namespace: IdNamespace,
        expires_at: std::time::SystemTime,
        resources: &super::super::GenerationResources<'_, R>,
    ) -> Result<g::GenerateContentResponseBody, TransformError> {
        self.publish_event(input, namespace, 0, expires_at, resources)
            .await
    }
    pub(crate) async fn publish_event<R: ResourceAccess<PublishedHandle = H>>(
        &mut self,
        input: &g::GenerateContentResponseBody,
        namespace: IdNamespace,
        event_index: u64,
        expires_at: std::time::SystemTime,
        resources: &super::super::GenerationResources<'_, R>,
    ) -> Result<g::GenerateContentResponseBody, TransformError> {
        if self.failed {
            return Err(conflict("image publication previously failed permanently"));
        }
        if expires_at <= resources.now {
            return Err(invalid("future image publication expiry required"));
        }
        let input = input.clone().into_declared();
        crate::codec::encode_json(&input, resources.limits).map_err(codec_error)?;
        if let Some(original) = &self.original {
            if original != &input {
                return Err(conflict(
                    "client image response changed during publication recovery",
                ));
            }
        } else {
            self.original = Some(input.clone());
        }
        let mut validated = Vec::new();
        let mut total = 0u64;
        for (candidate, value) in input.candidates.iter().flatten().enumerate() {
            for (index, part) in value
                .content
                .iter()
                .flat_map(|c| c.parts.iter().flatten())
                .enumerate()
            {
                if part.file_data.is_some() {
                    return Err(invalid(
                        "URI publication requires actual inline image bytes",
                    ));
                }
                let Some(blob) = &part.inline_data else {
                    continue;
                };
                if part.thought_signature.is_some() {
                    return Err(invalid(
                        "cannot replace a signed inline Part with a published fileData Part",
                    ));
                }
                let image = decode_image(
                    &blob.data,
                    Some(&blob.mime_type),
                    resources.limits.max_part_bytes,
                )?;
                total = total
                    .checked_add(image.bytes.len() as u64)
                    .ok_or_else(limit)?;
                if image.bytes.len() as u64 > resources.access.limits().write_bytes
                    || total > resources.limits.max_body_bytes
                    || validated.len() >= resources.max_references
                {
                    return Err(limit());
                }
                validated.push(((candidate, index), blob.clone(), image));
            }
        }
        let mut output = input;
        for (key, blob, image) in validated {
            let operation = format!(
                "generation-image:{}:{}:{}:{}",
                namespace.hex(),
                event_index,
                key.0,
                key.1
            );
            let metadata = ResourceMetadata {
                mime: Some(image.metadata.mime().into()),
                length: Some(image.bytes.len() as u64),
                filename: None,
                expires_at: Some(expires_at),
            };
            let recovering = self.publications.contains_key(&key);
            if let Some(known) = self.publications.get(&key) {
                if known.source != blob
                    || known.operation != operation
                    || known.expiry != expires_at
                {
                    return Err(conflict("publication identity or source image changed"));
                }
            } else {
                self.publications.insert(
                    key,
                    Publication {
                        source: blob,
                        operation: operation.clone(),
                        expiry: expires_at,
                        receipt: None,
                    },
                );
                // Retain the operation before the first await. Even an error may
                // leave an uncertain remote publication, which must be queried.
                let receipt = resources
                    .access
                    .publish(
                        resources.scope,
                        &operation,
                        PublicationKind::Url,
                        metadata.clone(),
                        HttpBody::Bytes(image.bytes),
                    )
                    .await?;
                let index = self.receipts.len();
                self.receipts.push(receipt);
                self.publications
                    .get_mut(&key)
                    .expect("reserved publication")
                    .receipt = Some(index);
            }
            if recovering {
                let receipt = match resources
                    .access
                    .publication_status(resources.scope, &operation)
                    .await?
                {
                    PublicationStatus::Published(receipt) => receipt,
                    PublicationStatus::Missing => {
                        return Err(missing(
                            "unacknowledged publication has no durable result; body cannot be replayed",
                        ));
                    }
                    PublicationStatus::Pending => {
                        return Err(conflict("image publication is still pending"));
                    }
                    PublicationStatus::Expired => return Err(missing("image publication expired")),
                };
                if let Some(index) = self.publications[&key].receipt {
                    let old = &self.receipts[index];
                    if old.reference != receipt.reference || old.metadata != receipt.metadata {
                        self.receipts.push(receipt);
                        self.failed = true;
                        return Err(conflict(
                            "acknowledged publication changed in supplied resource scope",
                        ));
                    }
                } else {
                    let index = self.receipts.len();
                    self.receipts.push(receipt);
                    self.publications
                        .get_mut(&key)
                        .expect("reserved publication")
                        .receipt = Some(index);
                }
            }
            let receipt = &self.receipts[self.publications[&key]
                .receipt
                .expect("acknowledged publication")];
            let ResourceReference::Url(url) = &receipt.reference else {
                self.failed = true;
                return Err(invalid("image publisher returned an ID instead of URL"));
            };
            if url.trim().is_empty()
                || receipt.metadata.mime != metadata.mime
                || receipt.metadata.length != metadata.length
                || receipt.metadata.expires_at != metadata.expires_at
            {
                self.failed = true;
                return Err(invalid(
                    "image publication metadata contradicts requested bytes or expiry",
                ));
            }
            let part = output.candidates.as_mut().expect("candidate")[key.0]
                .content
                .as_mut()
                .and_then(|content| content.parts.as_mut())
                .expect("parts")
                .get_mut(key.1)
                .expect("part");
            part.inline_data = None;
            part.file_data = Some(
                g::FileData::builder(url.clone())
                    .mime_type(image.metadata.mime())
                    .build(),
            );
        }
        crate::codec::encode_json(&output, resources.limits).map_err(codec_error)?;
        Ok(output)
    }
}

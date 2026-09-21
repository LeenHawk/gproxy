use super::*;
use crate::{
    capability::StateStore,
    transform::{
        generate::gemini_responses::RestoredGeminiImage,
        identity::{IdentityFlow, IdentityRole, IdentityStateRecord, OpaqueField, OutputItemKind},
    },
    wire::{DeclaredFields, openai::responses as r},
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct FileImageProof {
    schema: u16,
    conversation: String,
    state: IdentityStateRecord,
    part: g::Part,
    materialized: g::Blob,
    #[serde(default)]
    resource_expiry: Option<std::time::SystemTime>,
}

fn image_role() -> IdentityRole {
    IdentityRole::OutputItem(OutputItemKind::ImageGenerationCall)
}

impl<S: StateStore> super::super::GenerationStateAccess<'_, S> {
    pub(in crate::adapt::generate) async fn save_file_image_proofs(
        &self,
        images: &[MaterializedImage],
        client: &r::GenerateContentResponseBody,
        flow: &IdentityFlow,
        progress: &mut GenerationProgress<()>,
    ) -> Result<(), TransformError> {
        let mut payloads = Vec::new();
        for image in images
            .iter()
            .filter(|image| image.original.thought_signature.is_some())
        {
            if image.candidate != 0 {
                return Err(invalid(
                    "Responses image proof requires one native candidate",
                ));
            }
            let handle = flow
                .handles()
                .find(|handle| {
                    handle.role == image_role()
                        && handle.source.dialect == crate::Dialect::Gemini
                        && handle.source.logical_index == image.part as u64
                })
                .ok_or_else(|| missing("materialized file image has no exact output identity"))?;
            let id = handle.client_id();
            let item = client
                .output
                .iter()
                .find_map(|item| match item {
                    r::ResponseOutputItem::ImageGenerationCall(item) if item.id == id => Some(item),
                    _ => None,
                })
                .ok_or_else(|| missing("materialized image is absent from converted output"))?;
            if item.result.as_ref() != Some(&image.materialized.data) {
                return Err(conflict(
                    "converted image bytes differ from materialization proof",
                ));
            }
            let record = self.read(image_role(), id).await?.ok_or_else(|| {
                missing("image identity must be saved before materialization proof")
            })?;
            if record.opaque_signature.as_ref().is_none_or(|signature| {
                signature.field != OpaqueField::GeminiPartThoughtSignature
                    || Some(signature.value.as_str()) != image.original.thought_signature.as_deref()
            }) {
                return Err(conflict(
                    "saved identity does not bind the original fileData signature",
                ));
            }
            let proof = FileImageProof {
                schema: 1,
                conversation: self.conversation_key.clone(),
                state: record,
                part: image.original.clone().into_declared(),
                materialized: image.materialized.clone().into_declared(),
                resource_expiry: image.resource_expiry,
            };
            payloads.push((
                format!("{}:gemini-image-file", self.key(image_role(), id)?),
                serde_json::to_vec(&proof)?,
            ));
        }
        self.save_records(Vec::new(), payloads, progress).await
    }
    /// Called after the normal scoped identity and native Part have been read.
    /// The proof authenticates a materialized view; it never alters that Part.
    pub(in crate::adapt::generate) async fn gemini_file_image_replay(
        &self,
        id: &str,
        state: IdentityStateRecord,
        part: g::Part,
    ) -> Result<RestoredGeminiImage, TransformError> {
        let key = format!("{}:gemini-image-file", self.key(image_role(), id)?);
        let entry =
            self.store.get(self.scope, &key).await?.ok_or_else(|| {
                missing("signed fileData image materialization proof unavailable")
            })?;
        if entry.payload.len() as u64 > self.store.limits().read_bytes {
            return Err(limit());
        }
        if entry.expires_at.is_none_or(|expiry| expiry <= self.now) {
            return Err(missing("signed fileData image proof expired"));
        }
        let proof: FileImageProof = serde_json::from_slice(&entry.payload)?;
        if proof
            .resource_expiry
            .is_some_and(|expiry| expiry <= self.now)
        {
            return Err(missing("original signed fileData image resource expired"));
        }
        let original = part.into_declared();
        if proof.schema != 1
            || proof.state != state
            || proof.part.clone().into_declared() != original
            || state.role != image_role()
            || state.client_item_id.as_deref() != Some(id)
            || proof.conversation != self.conversation_key
            || original.file_data.is_none()
            || original.inline_data.is_some()
        {
            return Err(conflict(
                "signed fileData image proof differs from original identity or Part",
            ));
        }

        let materialized = proof.materialized.into_declared();
        crate::transform::images::decode_image(
            &materialized.data,
            Some(&materialized.mime_type),
            self.store.limits().read_bytes,
        )?;
        Ok(RestoredGeminiImage {
            state,
            part: original,
            materialized,
        })
    }
}

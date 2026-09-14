use super::*;
use crate::wire::{DeclaredFields, gemini as g};
impl<R: ResourceAccess> GenerationResources<'_, R> {
    pub async fn gemini(
        &self,
        input: g::GenerateContentRequestBody,
    ) -> Result<g::GenerateContentRequestBody, TransformError> {
        let mut input = input.into_declared();
        let mut budget = self.budget();
        for content in input
            .contents
            .iter_mut()
            .chain(input.system_instruction.iter_mut())
        {
            for part in content.parts.iter_mut().flatten() {
                if let Some(file) = &part.file_data {
                    if part.inline_data.is_some() {
                        return Err(TransformError::shape(
                            "part",
                            "ambiguous file and inline data",
                        ));
                    }
                    let media = budget
                        .read(ResourceReference::Url(file.file_uri.clone()), false)
                        .await?;
                    if file
                        .mime_type
                        .as_ref()
                        .is_some_and(|mime| mime != &media.mime)
                    {
                        return Err(TransformError::invalid_result(
                            "file_data.mime",
                            "resource MIME differs from request",
                        ));
                    }
                    part.inline_data =
                        Some(g::Blob::builder(media.mime, STANDARD.encode(media.bytes)).build());
                    part.file_data = None;
                }
            }
        }
        Ok(input)
    }
}

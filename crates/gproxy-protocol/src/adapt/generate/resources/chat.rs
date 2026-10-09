use super::*;
use crate::wire::{DeclaredFields, openai::chat as h};

impl<R: ResourceAccess> GenerationResources<'_, R> {
    pub async fn chat(
        &self,
        input: h::GenerateContentRequestBody,
    ) -> Result<h::GenerateContentRequestBody, TransformError> {
        let mut input = input.into_declared();
        let mut budget = self.budget(crate::Dialect::OpenAiChat);
        for message in &mut input.messages {
            if let h::ChatMessage::User(message) = message
                && let h::UserContent::Parts(parts) = &mut message.content
            {
                for part in parts {
                    match part {
                        h::UserContentPart::Image(part)
                            if !part.image_url.url.starts_with("data:") =>
                        {
                            let Some(media) = budget
                                .read(ResourceReference::Url(part.image_url.url.clone()), true)
                                .await?
                            else {
                                continue;
                            };
                            part.image_url.url = media.data_uri();
                        }
                        h::UserContentPart::File(part) if part.file.file_id.is_some() => {
                            if part.file.file_data.is_some() {
                                return Err(TransformError::shape(
                                    "file",
                                    "ambiguous inline data and native ID",
                                ));
                            }
                            let Some(media) = budget
                                .read(
                                    ResourceReference::Id(
                                        part.file.file_id.clone().expect("matched"),
                                    ),
                                    false,
                                )
                                .await?
                            else {
                                continue;
                            };
                            part.file.file_data = Some(media.data_uri());
                            part.file.file_id = None;
                            if part.file.filename.is_none() {
                                part.file.filename = media.filename;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        Ok(input)
    }
}

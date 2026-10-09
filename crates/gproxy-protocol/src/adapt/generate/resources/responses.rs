use super::*;
use crate::wire::{DeclaredFields, openai::responses as r};

impl<R: ResourceAccess> GenerationResources<'_, R> {
    pub async fn responses(
        &self,
        input: r::GenerateContentRequestBody,
    ) -> Result<r::GenerateContentRequestBody, TransformError> {
        let mut input = input.into_declared();
        let mut budget = self.budget(crate::Dialect::OpenAi);
        if let Some(r::Input::Items(items)) = &mut input.input {
            for item in items {
                match item {
                    r::InputItem::Message(message) => {
                        for part in &mut message.content {
                            budget.input_part(part).await?;
                        }
                    }
                    r::InputItem::Easy(message) => {
                        if let r::MessageContent::Parts(parts) = &mut message.content {
                            for part in parts {
                                budget.input_part(part).await?;
                            }
                        }
                    }
                    r::InputItem::FunctionCallOutput(output) => {
                        if let r::FunctionOutput::Content(parts) = &mut output.output {
                            for part in parts {
                                budget.function_part(part).await?;
                            }
                        }
                    }
                    r::InputItem::CustomToolCallOutput(output) => {
                        if let r::CustomOutput::Content(parts) = &mut output.output {
                            for part in parts {
                                budget.input_part(part).await?;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(input)
    }
}

impl<R: ResourceAccess> Budget<'_, '_, R> {
    async fn input_part(&mut self, part: &mut r::InputContent) -> Result<(), TransformError> {
        match part {
            r::InputContent::Image(image) => {
                let id = image.file_id.as_ref().and_then(|v| v.as_ref());
                let url = image.image_url.as_ref().and_then(|v| v.as_ref());
                if id.is_some() && url.is_some() {
                    return Err(TransformError::shape(
                        "input_image",
                        "ambiguous native ID and URL",
                    ));
                }
                let reference = if let Some(id) = id {
                    ResourceReference::Id(id.clone())
                } else if let Some(url) = url {
                    if url.starts_with("data:") {
                        return Ok(());
                    }
                    ResourceReference::Url(url.clone())
                } else {
                    return Ok(());
                };
                let Some(media) = self.read(reference, true).await? else {
                    return Ok(());
                };
                image.image_url = Some(Some(media.data_uri()));
                image.file_id = None;
            }
            r::InputContent::File(file) => {
                let id = file.file_id.as_ref().and_then(|v| v.as_ref());
                let url = file.file_url.as_ref();
                if usize::from(id.is_some())
                    + usize::from(url.is_some())
                    + usize::from(file.file_data.is_some())
                    > 1
                {
                    return Err(TransformError::shape(
                        "input_file",
                        "ambiguous file representations",
                    ));
                }
                let reference = if let Some(id) = id {
                    ResourceReference::Id(id.clone())
                } else if let Some(url) = url {
                    ResourceReference::Url(url.clone())
                } else {
                    return Ok(());
                };
                let Some(media) = self.read(reference, false).await? else {
                    return Ok(());
                };
                file.file_data = Some(media.data_uri());
                file.file_id = None;
                file.file_url = None;
                if file.filename.is_none() {
                    file.filename = media.filename;
                }
            }
            r::InputContent::Text(_) => {}
        }
        Ok(())
    }
}

impl<R: ResourceAccess> Budget<'_, '_, R> {
    async fn function_part(
        &mut self,
        part: &mut r::FunctionOutputContent,
    ) -> Result<(), TransformError> {
        match part {
            r::FunctionOutputContent::Image(image) => {
                let id = image.file_id.as_ref().and_then(|v| v.as_ref());
                let url = image.image_url.as_ref().and_then(|v| v.as_ref());
                if id.is_some() && url.is_some() {
                    return Err(TransformError::shape(
                        "function_output.image",
                        "ambiguous native ID and URL",
                    ));
                }
                let reference = if let Some(id) = id {
                    ResourceReference::Id(id.clone())
                } else if let Some(url) = url {
                    if url.starts_with("data:") {
                        return Ok(());
                    }
                    ResourceReference::Url(url.clone())
                } else {
                    return Ok(());
                };
                let Some(media) = self.read(reference, true).await? else {
                    return Ok(());
                };
                image.image_url = Some(Some(media.data_uri()));
                image.file_id = None;
            }
            r::FunctionOutputContent::File(file) => {
                let id = file.file_id.as_ref().and_then(|v| v.as_ref());
                let url = file.file_url.as_ref().and_then(|v| v.as_ref());
                let data = file.file_data.as_ref().and_then(|v| v.as_ref());
                if usize::from(id.is_some())
                    + usize::from(url.is_some())
                    + usize::from(data.is_some())
                    > 1
                {
                    return Err(TransformError::shape(
                        "function_output.file",
                        "ambiguous file representations",
                    ));
                }
                let reference = if let Some(id) = id {
                    ResourceReference::Id(id.clone())
                } else if let Some(url) = url {
                    ResourceReference::Url(url.clone())
                } else {
                    return Ok(());
                };
                let Some(media) = self.read(reference, false).await? else {
                    return Ok(());
                };
                file.file_data = Some(Some(media.data_uri()));
                file.file_id = None;
                file.file_url = None;
                if file.filename.as_ref().is_none_or(|v| v.is_none()) {
                    file.filename = media.filename.map(Some);
                }
            }
            r::FunctionOutputContent::Text(_) => {}
        }
        Ok(())
    }
}

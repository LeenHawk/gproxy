use super::*;
use crate::wire::{
    DeclaredFields,
    claude::{content as c, generate_content as cg},
};

impl<R: ResourceAccess> GenerationResources<'_, R> {
    pub async fn claude(
        &self,
        input: cg::GenerateContentRequestBody,
    ) -> Result<cg::GenerateContentRequestBody, TransformError> {
        let mut input = input.into_declared();
        let mut budget = self.budget();
        for message in &mut input.messages {
            if let c::MessageContent::Blocks(blocks) = &mut message.content {
                for block in blocks {
                    match block {
                        c::ContentBlock::Image(image) => budget.image(image).await?,
                        c::ContentBlock::Document(doc) => budget.document(doc).await?,
                        c::ContentBlock::ToolResult(result) => {
                            if let Some(c::ToolResultContent::Blocks(blocks)) = &mut result.content
                            {
                                for block in blocks {
                                    match block {
                                        c::ToolResultContentBlock::Image(image) => {
                                            budget.image(image).await?
                                        }
                                        c::ToolResultContentBlock::Document(doc) => {
                                            budget.document(doc).await?
                                        }
                                        _ => {}
                                    }
                                }
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

impl<R: ResourceAccess> Budget<'_, '_, R> {
    async fn image(&mut self, image: &mut c::ImageBlock) -> Result<(), TransformError> {
        let reference = match &image.source {
            c::ImageSource::Url(source) => ResourceReference::Url(source.url.clone()),
            c::ImageSource::File(source) => ResourceReference::Id(source.file_id.clone()),
            c::ImageSource::Base64(_) => return Ok(()),
        };
        let media = self.read(reference, true).await?;
        let mime = match media.mime.as_str() {
            "image/png" => c::ImageMediaType::Png,
            "image/jpeg" => c::ImageMediaType::Jpeg,
            "image/gif" => c::ImageMediaType::Gif,
            "image/webp" => c::ImageMediaType::Webp,
            _ => {
                return Err(TransformError::unsupported(
                    "image.mime",
                    "Claude has no native image variant for this MIME",
                ));
            }
        };
        image.source = c::ImageSource::Base64(
            c::Base64Source::builder(STANDARD.encode(media.bytes), mime).build(),
        );
        Ok(())
    }
    async fn document(&mut self, doc: &mut c::DocumentBlock) -> Result<(), TransformError> {
        let reference = match &mut doc.source {
            c::DocumentSource::Url(source) => ResourceReference::Url(source.url.clone()),
            c::DocumentSource::File(source) => ResourceReference::Id(source.file_id.clone()),
            c::DocumentSource::Content(source) => {
                if let c::DocumentContent::Blocks(blocks) = &mut source.content {
                    for block in blocks {
                        if let c::DocumentContentBlock::Image(image) = block {
                            self.image(image).await?;
                        }
                    }
                }
                return Ok(());
            }
            _ => return Ok(()),
        };
        let media = self.read(reference, false).await?;
        doc.source = match media.mime.as_str() {
            "application/pdf" => {
                if !media.bytes.starts_with(b"%PDF-") {
                    return Err(TransformError::invalid_result(
                        "document.mime",
                        "PDF metadata does not match bytes",
                    ));
                }
                c::DocumentSource::Base64(
                    c::PdfBase64Source::builder(STANDARD.encode(media.bytes), c::PdfMediaType::Pdf)
                        .build(),
                )
            }
            "text/plain" => c::DocumentSource::Text(
                c::PlainTextSource::builder(
                    String::from_utf8(media.bytes.to_vec()).map_err(|e| {
                        TransformError::invalid_result("document.utf8", e.to_string())
                    })?,
                    c::TextMediaType::Plain,
                )
                .build(),
            ),
            _ => {
                return Err(TransformError::unsupported(
                    "document.mime",
                    "Claude document requires PDF or plain text",
                ));
            }
        };
        Ok(())
    }
}

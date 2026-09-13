use crate::{
    transform::TransformError,
    wire::{claude::content as c, openai::responses::input as r},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
pub(super) fn image(source: c::ImageSource) -> Result<r::InputContent, TransformError> {
    let url = match source {
        c::ImageSource::Url(source) => source.url,
        c::ImageSource::Base64(source) => format!(
            "data:{};base64,{}",
            match source.media_type {
                c::ImageMediaType::Jpeg => "image/jpeg",
                c::ImageMediaType::Png => "image/png",
                c::ImageMediaType::Gif => "image/gif",
                c::ImageMediaType::Webp => "image/webp",
            },
            source.data
        ),
        c::ImageSource::File(_) => {
            return Err(TransformError::missing_metadata(
                "Claude image file requires target resource mapping",
            ));
        }
    };
    let mut value = r::ResponseInputImage::builder(
        r::ResponseInputImageType::ResponseInputImage,
        r::ImageDetail::Auto,
    )
    .build();
    value.image_url = Some(Some(url));
    Ok(r::InputContent::Image(value))
}
pub(super) fn document(block: c::DocumentBlock) -> Result<r::InputContent, TransformError> {
    if block
        .citations
        .as_ref()
        .is_some_and(|v| v.enabled == Some(true))
        || block.context.is_some()
    {
        return Err(TransformError::unsupported(
            "document.context/citations",
            "Responses file lacks Claude document instructions",
        ));
    }
    let mut value =
        r::ResponseInputFile::builder(r::ResponseInputFileType::ResponseInputFile).build();
    value.filename = block.title;
    match block.source {
        c::DocumentSource::Url(source) => value.file_url = Some(source.url),
        c::DocumentSource::Base64(source) => {
            value.file_data = Some(format!("data:application/pdf;base64,{}", source.data))
        }
        c::DocumentSource::Text(source) => {
            value.file_data = Some(format!(
                "data:text/plain;base64,{}",
                STANDARD.encode(source.data)
            ))
        }
        c::DocumentSource::Content(source) => {
            let text = match source.content {
                c::DocumentContent::Text(text) => text,
                c::DocumentContent::Blocks(blocks) => blocks
                    .into_iter()
                    .map(|block| match block {
                        c::DocumentContentBlock::Text(text) => Ok(text.text),
                        c::DocumentContentBlock::Image(_) => Err(TransformError::missing_metadata(
                            "mixed document resource conversion",
                        )),
                    })
                    .collect::<Result<Vec<_>, _>>()?
                    .join(""),
            };
            value.file_data = Some(format!("data:text/plain;base64,{}", STANDARD.encode(text)));
        }
        c::DocumentSource::File(_) => {
            return Err(TransformError::missing_metadata(
                "Claude document file requires target resource mapping",
            ));
        }
    }
    Ok(r::InputContent::File(value))
}
pub(super) fn to_claude(value: r::InputContent) -> Result<c::ContentBlock, TransformError> {
    Ok(match value {
        r::InputContent::Text(value) => {
            c::ContentBlock::Text(c::TextBlock::builder(c::TextBlockType::Tag, value.text).build())
        }
        r::InputContent::Image(value) => {
            if value.file_id.flatten().is_some() {
                return Err(TransformError::missing_metadata(
                    "Responses image file requires Claude resource mapping",
                ));
            }
            let url = value
                .image_url
                .flatten()
                .ok_or_else(|| TransformError::missing_metadata("image_url"))?;
            let source = if let Some(data) = url.strip_prefix("data:") {
                let (mime, bytes) = data
                    .split_once(";base64,")
                    .ok_or_else(|| TransformError::shape("image_url", "invalid data URI"))?;
                STANDARD
                    .decode(bytes)
                    .map_err(|e| TransformError::shape("image_url", e.to_string()))?;
                c::ImageSource::Base64(
                    c::Base64Source::builder(
                        bytes.into(),
                        match mime {
                            "image/jpeg" => c::ImageMediaType::Jpeg,
                            "image/png" => c::ImageMediaType::Png,
                            "image/gif" => c::ImageMediaType::Gif,
                            "image/webp" => c::ImageMediaType::Webp,
                            _ => {
                                return Err(TransformError::unsupported(
                                    "image.mime",
                                    "unsupported Claude image MIME",
                                ));
                            }
                        },
                    )
                    .build(),
                )
            } else {
                c::ImageSource::Url(c::UrlSource::builder(url).build())
            };
            c::ContentBlock::Image(c::ImageBlock::builder(c::ImageBlockType::Tag, source).build())
        }
        r::InputContent::File(value) => {
            if value.file_id.flatten().is_some() {
                return Err(TransformError::missing_metadata(
                    "Responses file requires Claude resource mapping",
                ));
            }
            let source = if let Some(url) = value.file_url {
                c::DocumentSource::Url(c::UrlSource::builder(url).build())
            } else {
                let data = value
                    .file_data
                    .ok_or_else(|| TransformError::missing_metadata("file_data"))?;
                if let Some(bytes) = data.strip_prefix("data:application/pdf;base64,") {
                    STANDARD
                        .decode(bytes)
                        .map_err(|e| TransformError::shape("file_data", e.to_string()))?;
                    c::DocumentSource::Base64(
                        c::PdfBase64Source::builder(bytes.into(), c::PdfMediaType::Pdf).build(),
                    )
                } else if let Some(bytes) = data.strip_prefix("data:text/plain;base64,") {
                    let bytes = STANDARD
                        .decode(bytes)
                        .map_err(|e| TransformError::shape("file_data", e.to_string()))?;
                    c::DocumentSource::Text(
                        c::PlainTextSource::builder(
                            String::from_utf8(bytes)
                                .map_err(|e| TransformError::shape("file_data", e.to_string()))?,
                            c::TextMediaType::Plain,
                        )
                        .build(),
                    )
                } else {
                    return Err(TransformError::missing_metadata("file_data MIME/data URI"));
                }
            };
            let mut block = c::DocumentBlock::builder(c::DocumentBlockType::Tag, source).build();
            block.title = value.filename;
            c::ContentBlock::Document(block)
        }
    })
}
pub(super) fn result_to_responses(
    value: c::ToolResultContent,
) -> Result<r::FunctionOutput, TransformError> {
    Ok(match value {
        c::ToolResultContent::Text(text) => r::FunctionOutput::Text(text),
        c::ToolResultContent::Blocks(blocks) => r::FunctionOutput::Content(
            blocks
                .into_iter()
                .map(|block| {
                    let part = match block {
                        c::ToolResultContentBlock::Text(text) => {
                            return Ok(r::FunctionOutputContent::Text(
                                r::FunctionOutputText::builder(
                                    r::ResponseInputTextType::ResponseInputText,
                                    text.text,
                                )
                                .build(),
                            ));
                        }
                        c::ToolResultContentBlock::Image(block) => image(block.source)?,
                        c::ToolResultContentBlock::Document(doc) => document(doc)?,
                        c::ToolResultContentBlock::SearchResult(_)
                        | c::ToolResultContentBlock::ToolReference(_) => {
                            return Err(TransformError::unsupported(
                                "tool_result",
                                "search/tool-reference result needs target metadata mapping",
                            ));
                        }
                    };
                    Ok(match part {
                        r::InputContent::Image(value) => {
                            let mut target = r::FunctionOutputImage::builder(
                                r::ResponseInputImageType::ResponseInputImage,
                            )
                            .build();
                            target.image_url = value.image_url;
                            target.detail = Some(Some(value.detail));
                            r::FunctionOutputContent::Image(target)
                        }
                        r::InputContent::File(value) => {
                            let mut target = r::FunctionOutputFile::builder(
                                r::ResponseInputFileType::ResponseInputFile,
                            )
                            .build();
                            target.file_data = value.file_data.map(Some);
                            target.file_url = value.file_url.map(Some);
                            target.filename = value.filename.map(Some);
                            r::FunctionOutputContent::File(target)
                        }
                        r::InputContent::Text(_) => unreachable!(),
                    })
                })
                .collect::<Result<Vec<_>, TransformError>>()?,
        ),
    })
}
pub(super) fn result_to_claude(
    value: r::FunctionOutput,
) -> Result<c::ToolResultContent, TransformError> {
    Ok(match value {
        r::FunctionOutput::Text(text) => c::ToolResultContent::Text(text),
        r::FunctionOutput::Content(parts) => c::ToolResultContent::Blocks(
            parts
                .into_iter()
                .map(|part| {
                    let input = match part {
                        r::FunctionOutputContent::Text(text) => {
                            return Ok(c::ToolResultContentBlock::Text(
                                c::TextBlock::builder(c::TextBlockType::Tag, text.text).build(),
                            ));
                        }
                        r::FunctionOutputContent::Image(value) => {
                            let mut input = r::ResponseInputImage::builder(
                                r::ResponseInputImageType::ResponseInputImage,
                                value.detail.flatten().unwrap_or(r::ImageDetail::Auto),
                            )
                            .build();
                            input.image_url = value.image_url;
                            input.file_id = value.file_id;
                            r::InputContent::Image(input)
                        }
                        r::FunctionOutputContent::File(value) => {
                            let mut input = r::ResponseInputFile::builder(
                                r::ResponseInputFileType::ResponseInputFile,
                            )
                            .build();
                            input.file_data = value.file_data.flatten();
                            input.file_url = value.file_url.flatten();
                            input.file_id = value.file_id;
                            input.filename = value.filename.flatten();
                            r::InputContent::File(input)
                        }
                    };
                    match to_claude(input)? {
                        c::ContentBlock::Image(image) => {
                            Ok(c::ToolResultContentBlock::Image(image))
                        }
                        c::ContentBlock::Document(doc) => {
                            Ok(c::ToolResultContentBlock::Document(doc))
                        }
                        c::ContentBlock::Text(_)
                        | c::ContentBlock::Thinking(_)
                        | c::ContentBlock::RedactedThinking(_)
                        | c::ContentBlock::ToolUse(_)
                        | c::ContentBlock::ToolResult(_)
                        | c::ContentBlock::ServerToolUse(_)
                        | c::ContentBlock::SearchResult(_)
                        | c::ContentBlock::WebSearchToolResult(_)
                        | c::ContentBlock::WebFetchToolResult(_)
                        | c::ContentBlock::AdvisorToolResult(_)
                        | c::ContentBlock::CodeExecutionToolResult(_)
                        | c::ContentBlock::BashCodeExecutionToolResult(_)
                        | c::ContentBlock::TextEditorCodeExecutionToolResult(_)
                        | c::ContentBlock::ToolSearchToolResult(_)
                        | c::ContentBlock::McpToolUse(_)
                        | c::ContentBlock::McpToolResult(_)
                        | c::ContentBlock::ContainerUpload(_)
                        | c::ContentBlock::Compaction(_)
                        | c::ContentBlock::MidConversationSystem(_)
                        | c::ContentBlock::ToolAddition(_)
                        | c::ContentBlock::ToolRemoval(_)
                        | c::ContentBlock::Fallback(_) => {
                            unreachable!("media helper returns only image/document")
                        }
                    }
                })
                .collect::<Result<Vec<_>, TransformError>>()?,
        ),
    })
}

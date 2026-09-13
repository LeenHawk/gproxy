use crate::{
    transform::{TransformError, memory::MemoryDialectRequest as Target},
    wire::{
        claude::content as a,
        gemini as g,
        openai::{chat as c, guardian as source, responses as r},
    },
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
pub(super) fn attach(
    target: &mut Target,
    history: &[source::ClientResponseItem],
) -> Result<(), TransformError> {
    for item in history {
        if let source::ClientResponseItem::Message(message) = item {
            for part in &message.content {
                match part {
                    source::ContentItem::InputImage(image) => attach_image(target, image)?,
                    source::ContentItem::InputAudio(audio) => {
                        attach_audio(target, &audio.audio_url)?
                    }
                    source::ContentItem::InputText(_) | source::ContentItem::OutputText(_) => {}
                }
            }
        }
    }
    Ok(())
}
fn uri(value: &str) -> Result<(String, String), TransformError> {
    let (mime, data) = value
        .strip_prefix("data:")
        .and_then(|v| v.split_once(";base64,"))
        .ok_or_else(|| {
            TransformError::missing_metadata("compact media requires resolved data URI with MIME")
        })?;
    STANDARD
        .decode(data)
        .map_err(|e| TransformError::shape("compact.media", e.to_string()))?;
    Ok((mime.into(), data.into()))
}
fn attach_image(
    target: &mut Target,
    image: &source::ContentItemInputImage,
) -> Result<(), TransformError> {
    let detail = match image.detail {
        None | Some(source::ImageDetail::Auto) => r::ImageDetail::Auto,
        Some(source::ImageDetail::Low) => r::ImageDetail::Low,
        Some(source::ImageDetail::High) => r::ImageDetail::High,
        Some(source::ImageDetail::Original) => r::ImageDetail::Original,
    };
    match target {
        Target::OpenAiResponses(request) => {
            let mut value = r::ResponseInputImage::builder(
                r::ResponseInputImageType::ResponseInputImage,
                detail,
            )
            .build();
            value.image_url = Some(Some(image.image_url.clone()));
            let parts = vec![r::InputContent::Image(value)];
            let existing = request.body.input.take();
            let mut items = match existing {
                Some(r::Input::Text(text)) => vec![r::InputItem::Easy(
                    r::EasyInputMessage::builder(
                        r::MessageContent::Text(text),
                        r::MessageRole::User,
                    )
                    .build(),
                )],
                Some(r::Input::Items(items)) => items,
                None => Vec::new(),
            };
            items.push(r::InputItem::Easy(
                r::EasyInputMessage::builder(r::MessageContent::Parts(parts), r::MessageRole::User)
                    .build(),
            ));
            request.body.input = Some(r::Input::Items(items));
        }
        Target::OpenAiChat(request) => {
            let detail = match detail {
                r::ImageDetail::Auto => c::ImageDetail::Auto,
                r::ImageDetail::Low => c::ImageDetail::Low,
                r::ImageDetail::High => c::ImageDetail::High,
                r::ImageDetail::Original => {
                    return Err(TransformError::unsupported(
                        "compact.image.detail",
                        "Chat lacks original resolution",
                    ));
                }
            };
            request.body.messages.push(c::ChatMessage::User(
                c::UserMessage::builder(
                    c::UserRole::User,
                    c::UserContent::Parts(vec![c::UserContentPart::Image(
                        c::ImagePart::builder(
                            c::ImagePartType::ImageUrl,
                            c::ImageUrl::builder(image.image_url.clone())
                                .detail(detail)
                                .build(),
                        )
                        .build(),
                    )]),
                )
                .build(),
            ));
        }
        Target::Claude(request) => {
            if detail != r::ImageDetail::Auto {
                return Err(TransformError::unsupported(
                    "compact.image.detail",
                    "Claude lacks matching image detail selector",
                ));
            }
            let source = if image.image_url.starts_with("data:") {
                let (mime, data) = uri(&image.image_url)?;
                a::ImageSource::Base64(
                    a::Base64Source::builder(
                        data,
                        match mime.as_str() {
                            "image/jpeg" => a::ImageMediaType::Jpeg,
                            "image/png" => a::ImageMediaType::Png,
                            "image/gif" => a::ImageMediaType::Gif,
                            "image/webp" => a::ImageMediaType::Webp,
                            _ => {
                                return Err(TransformError::unsupported(
                                    "compact.image.mime",
                                    "unsupported Claude MIME",
                                ));
                            }
                        },
                    )
                    .build(),
                )
            } else {
                a::ImageSource::Url(a::UrlSource::builder(image.image_url.clone()).build())
            };
            request.body.messages.push(
                a::Message::builder(
                    a::Role::User,
                    a::MessageContent::Blocks(vec![a::ContentBlock::Image(
                        a::ImageBlock::builder(a::ImageBlockType::Tag, source).build(),
                    )]),
                )
                .build(),
            );
        }
        Target::Gemini(request) => {
            if detail != r::ImageDetail::Auto {
                return Err(TransformError::unsupported(
                    "compact.image.detail",
                    "Gemini needs explicit resolution mapping",
                ));
            }
            let (mime, data) = uri(&image.image_url)?;
            request.body.contents.push(
                g::Content::builder()
                    .role("user")
                    .parts(vec![
                        g::Part::builder()
                            .inline_data(g::Blob::builder(mime, data).build())
                            .build(),
                    ])
                    .build(),
            );
        }
    }
    Ok(())
}
fn attach_audio(target: &mut Target, url: &str) -> Result<(), TransformError> {
    let (mime, data) = uri(url)?;
    match target {
        Target::OpenAiChat(request) => {
            let format = match mime.as_str() {
                "audio/wav" => c::AudioFormat::Wav,
                "audio/mpeg" | "audio/mp3" => c::AudioFormat::Mp3,
                _ => {
                    return Err(TransformError::unsupported(
                        "compact.audio.mime",
                        "Chat supports WAV/MP3",
                    ));
                }
            };
            request.body.messages.push(c::ChatMessage::User(
                c::UserMessage::builder(
                    c::UserRole::User,
                    c::UserContent::Parts(vec![c::UserContentPart::InputAudio(
                        c::InputAudioPart::builder(
                            c::InputAudioPartType::InputAudio,
                            c::InputAudio::builder(data, format).build(),
                        )
                        .build(),
                    )]),
                )
                .build(),
            ));
        }
        Target::Gemini(request) => request.body.contents.push(
            g::Content::builder()
                .role("user")
                .parts(vec![
                    g::Part::builder()
                        .inline_data(g::Blob::builder(mime, data).build())
                        .build(),
                ])
                .build(),
        ),
        Target::Claude(_) | Target::OpenAiResponses(_) => {
            return Err(TransformError::unsupported(
                "compact.audio",
                "target requires a transcription capability",
            ));
        }
    }
    Ok(())
}

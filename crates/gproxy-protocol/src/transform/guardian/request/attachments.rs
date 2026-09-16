use super::*;

pub(crate) fn attach_media(
    target: &mut GuardianDialectRequest,
    media: &[Media],
) -> Result<(), TransformError> {
    for media in media {
        match (&mut *target, media) {
            (
                GuardianDialectRequest::OpenAiChat(request),
                Media::Image {
                    url,
                    detail: selector,
                },
            ) => {
                let selector = match detail(selector) {
                    r::ImageDetail::Auto => Some(o::ImageDetail::Auto),
                    r::ImageDetail::Low => Some(o::ImageDetail::Low),
                    r::ImageDetail::High => Some(o::ImageDetail::High),
                    r::ImageDetail::Original => None,
                };
                let mut image = o::ImageUrl::builder(url.clone()).build();
                image.detail = selector;
                request.body.messages.push(o::ChatMessage::User(
                    o::UserMessage::builder(
                        o::UserRole::User,
                        o::UserContent::Parts(vec![o::UserContentPart::Image(
                            o::ImagePart::builder(o::ImagePartType::ImageUrl, image).build(),
                        )]),
                    )
                    .build(),
                ));
            }
            (
                GuardianDialectRequest::OpenAiResponses(request),
                Media::Image {
                    url,
                    detail: selector,
                },
            ) => {
                let image = r::ResponseInputImage::builder(
                    r::ResponseInputImageType::ResponseInputImage,
                    detail(selector),
                )
                .build();
                let mut image = image;
                image.image_url = Some(Some(url.clone()));
                let mut items = match request.body.input.take() {
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
                    r::EasyInputMessage::builder(
                        r::MessageContent::Parts(vec![r::InputContent::Image(image)]),
                        r::MessageRole::User,
                    )
                    .build(),
                ));
                request.body.input = Some(r::Input::Items(items));
            }
            (
                GuardianDialectRequest::Claude(request),
                Media::Image {
                    url,
                    detail: _selector,
                },
            ) => {
                let source = if url.starts_with("data:") {
                    let (mime, data) = data_uri(url)?;
                    cc::ImageSource::Base64(
                        cc::Base64Source::builder(
                            data,
                            match mime.as_str() {
                                "image/jpeg" => cc::ImageMediaType::Jpeg,
                                "image/png" => cc::ImageMediaType::Png,
                                "image/gif" => cc::ImageMediaType::Gif,
                                "image/webp" => cc::ImageMediaType::Webp,
                                _ => {
                                    continue;
                                }
                            },
                        )
                        .build(),
                    )
                } else {
                    cc::ImageSource::Url(cc::UrlSource::builder(url.clone()).build())
                };
                request.body.messages.push(
                    cc::Message::builder(
                        cc::Role::User,
                        cc::MessageContent::Blocks(vec![cc::ContentBlock::Image(
                            cc::ImageBlock::builder(cc::ImageBlockType::Tag, source).build(),
                        )]),
                    )
                    .build(),
                );
            }
            (
                GuardianDialectRequest::Gemini(request),
                Media::Image {
                    url,
                    detail: _selector,
                },
            ) => {
                let (mime, data) = data_uri(url)?;
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
            (GuardianDialectRequest::OpenAiChat(request), Media::Audio { url }) => {
                let (mime, data) = data_uri(url)?;
                let format = match mime.as_str() {
                    "audio/wav" => o::AudioFormat::Wav,
                    "audio/mpeg" | "audio/mp3" => o::AudioFormat::Mp3,
                    _ => {
                        continue;
                    }
                };
                request.body.messages.push(o::ChatMessage::User(
                    o::UserMessage::builder(
                        o::UserRole::User,
                        o::UserContent::Parts(vec![o::UserContentPart::InputAudio(
                            o::InputAudioPart::builder(
                                o::InputAudioPartType::InputAudio,
                                o::InputAudio::builder(data, format).build(),
                            )
                            .build(),
                        )]),
                    )
                    .build(),
                ));
            }
            (GuardianDialectRequest::Gemini(request), Media::Audio { url }) => {
                let (mime, data) = data_uri(url)?;
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
            (_, Media::Audio { .. }) => {
                continue;
            }
        }
    }
    Ok(())
}

use crate::{
    transform::TransformError,
    wire::{gemini as g, openai::chat as c},
};

pub(super) fn to_chat(blob: g::Blob) -> Result<c::UserContentPart, TransformError> {
    Ok(if blob.mime_type.starts_with("image/") {
        c::UserContentPart::Image(
            c::ImagePart::builder(
                c::ImagePartType::ImageUrl,
                c::ImageUrl::builder(format!("data:{};base64,{}", blob.mime_type, blob.data))
                    .build(),
            )
            .build(),
        )
    } else if matches!(
        blob.mime_type.as_str(),
        "audio/wav" | "audio/x-wav" | "audio/mpeg" | "audio/mp3"
    ) {
        let format = if matches!(blob.mime_type.as_str(), "audio/wav" | "audio/x-wav") {
            c::AudioFormat::Wav
        } else {
            c::AudioFormat::Mp3
        };
        c::UserContentPart::InputAudio(
            c::InputAudioPart::builder(
                c::InputAudioPartType::InputAudio,
                c::InputAudio::builder(blob.data, format).build(),
            )
            .build(),
        )
    } else if blob.mime_type == "application/pdf" || blob.mime_type == "text/plain" {
        let mut file = c::FileRef::builder().build();
        file.file_data = Some(format!("data:{};base64,{}", blob.mime_type, blob.data));
        c::UserContentPart::File(c::FilePart::builder(c::FilePartType::File, file).build())
    } else {
        return Err(TransformError::unsupported(
            "inline_data.mime_type",
            "Chat has no equivalent media part for this MIME",
        ));
    })
}

pub(super) fn data_uri(value: &str) -> Result<g::Blob, TransformError> {
    let (mime, data) = value
        .strip_prefix("data:")
        .and_then(|v| v.split_once(";base64,"))
        .ok_or_else(|| {
            TransformError::missing_metadata("media URL requires resource bytes and exact MIME")
        })?;

    if mime.is_empty() {
        return Err(TransformError::shape("media.mime", "missing MIME"));
    }
    Ok(g::Blob::builder(mime.into(), data.into()).build())
}

pub(super) fn user(content: &c::UserContent) -> Result<Vec<g::Part>, TransformError> {
    match content {
        c::UserContent::Text(text) => Ok(vec![g::Part::builder().text(text.clone()).build()]),
        c::UserContent::Parts(parts) => parts
            .iter()
            .map(|part| {
                Ok(match part {
                    c::UserContentPart::Text(part) => {
                        g::Part::builder().text(part.text.clone()).build()
                    }
                    c::UserContentPart::Image(part) => g::Part::builder()
                        .inline_data(data_uri(&part.image_url.url)?)
                        .build(),
                    c::UserContentPart::InputAudio(part) => g::Part::builder()
                        .inline_data(
                            g::Blob::builder(
                                match part.input_audio.format {
                                    c::AudioFormat::Wav => "audio/wav",
                                    c::AudioFormat::Mp3 => "audio/mpeg",
                                }
                                .into(),
                                part.input_audio.data.clone(),
                            )
                            .build(),
                        )
                        .build(),
                    c::UserContentPart::File(part) => {
                        let data = part.file.file_data.as_deref().ok_or_else(|| {
                            TransformError::missing_metadata("file_id requires resource access")
                        })?;
                        g::Part::builder().inline_data(data_uri(data)?).build()
                    }
                })
            })
            .filter_map(|value| crate::transform::optional(value).transpose())
            .collect(),
    }
}

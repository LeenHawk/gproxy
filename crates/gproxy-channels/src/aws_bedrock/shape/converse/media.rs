use gproxy_channel_api::ChannelError;
use gproxy_protocol::{aws, claude};
use serde_json::Value;

pub(super) fn image(source: claude::ImageSource) -> Result<aws::ImageBlock, ChannelError> {
    let claude::ImageSource::Base64(source) = source else {
        return Err(super::content::prepare(
            "Bedrock image source must be base64",
        ));
    };
    let media: String = super::content::transcode(source.media_type, "image media type")?;
    let format = media
        .rsplit('/')
        .next()
        .unwrap_or("png")
        .replace("jpg", "jpeg");
    Ok(aws::ImageBlock {
        format: serde_json::from_value(Value::String(format))
            .map_err(|error| super::content::prepare(error.to_string()))?,
        source: aws::ImageSource::Bytes {
            bytes: source.data,
            rest: source.rest,
        },
        rest: Default::default(),
    })
}

pub(super) fn document(
    source: claude::DocumentSource,
    title: Option<String>,
    context: Option<String>,
) -> Result<aws::DocumentBlock, ChannelError> {
    let (format, source) = match source {
        claude::DocumentSource::Base64(source) => (
            Some(aws::DocumentFormat::Known(aws::DocumentFormatKnown::Pdf)),
            aws::DocumentSource::Bytes {
                bytes: source.data,
                rest: source.rest,
            },
        ),
        claude::DocumentSource::Text(source) => (
            Some(aws::DocumentFormat::Known(aws::DocumentFormatKnown::Txt)),
            aws::DocumentSource::Text {
                text: source.data,
                rest: source.rest,
            },
        ),
        _ => {
            return Err(super::content::prepare(
                "Bedrock document source must be base64 PDF or text",
            ));
        }
    };
    Ok(aws::DocumentBlock {
        format,
        name: title.unwrap_or_else(|| "document".into()),
        source,
        context,
        rest: Default::default(),
    })
}

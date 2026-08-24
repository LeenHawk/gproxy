use base64::Engine as _;
use gproxy_channel_api::ChannelError;
use serde_json::Value;

pub(super) fn collect_image(
    source: Option<&Value>,
    uploads: &mut Vec<super::request::Upload>,
) -> Result<(), ChannelError> {
    let source = source.ok_or_else(|| ChannelError::Prepare("image source missing".into()))?;
    let media = source
        .get("media_type")
        .and_then(Value::as_str)
        .unwrap_or("image/png");
    let data = source
        .get("data")
        .and_then(Value::as_str)
        .or_else(|| {
            source
                .get("url")
                .and_then(Value::as_str)?
                .split_once(',')
                .map(|(_, data)| data)
        })
        .ok_or_else(|| ChannelError::Prepare("image must be a base64 data URL".into()))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|error| ChannelError::Prepare(format!("image base64: {error}")))?;
    let extension = media
        .split('/')
        .nth(1)
        .unwrap_or("png")
        .split(';')
        .next()
        .unwrap_or("png");
    uploads.push(super::request::Upload {
        bytes,
        media_type: media.into(),
        file_name: format!("image.{extension}"),
    });
    Ok(())
}

use super::*;
pub(crate) fn pcm_mime(format: &AudioFormat) -> Result<String> {
    match format {
        AudioFormat::Encoded { media_type } => Ok(media_type.clone()),
        AudioFormat::Pcm {
            media_type,
            sample_rate,
            channels,
        } => Ok(format!(
            "{media_type};rate={sample_rate};channels={channels}"
        )),
        AudioFormat::Unknown(_) => Err(Error::invalid("unknown audio format")),
    }
}
#[cfg(feature = "speech")]
pub(crate) fn parse_format(mime: &str) -> Result<AudioFormat> {
    let base = mime.split(';').next().unwrap_or(mime);
    if !matches!(base, "audio/pcm" | "audio/L16" | "audio/l16") {
        return Ok(AudioFormat::Encoded {
            media_type: mime.into(),
        });
    }
    let mut rate = None;
    let mut channels = None;
    for field in mime.split(';').skip(1) {
        if let Some(value) = field.trim().strip_prefix("rate=") {
            rate = Some(value.parse().map_err(Error::invalid)?);
        }
        if let Some(value) = field.trim().strip_prefix("channels=") {
            channels = Some(value.parse().map_err(Error::invalid)?);
        }
    }
    Ok(AudioFormat::Pcm {
        media_type: base.into(),
        sample_rate: rate.ok_or_else(|| Error::invalid("PCM output MIME has no sample rate"))?,
        channels: channels.unwrap_or(1),
    })
}

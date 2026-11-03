use super::*;
pub(crate) fn audio_reference_valid(block: &Block, now: u64) -> Result<bool> {
    Ok(metadata(block)?
        .and_then(|meta| meta.audio)
        .is_some_and(|audio| {
            audio.id.is_some()
                && audio.expires_at.is_some_and(|expiry| {
                    expiry > 0 && u64::try_from(expiry).is_ok_and(|expiry| expiry > now)
                })
        }))
}

pub(super) fn native_format(format: &AudioFormat) -> Result<q::ChatAudioFormat> {
    Ok(match format {
        AudioFormat::Unknown(_) => return Err(Error::invalid("unknown persisted audio format")),
        AudioFormat::Pcm {
            sample_rate: 24000,
            channels: 1,
            ..
        } => q::ChatAudioFormat::Pcm16,
        AudioFormat::Encoded { media_type } => match media_type.as_str() {
            "audio/wav" => q::ChatAudioFormat::Wav,
            "audio/mpeg" | "audio/mp3" => q::ChatAudioFormat::Mp3,
            "audio/aac" => q::ChatAudioFormat::Aac,
            "audio/flac" => q::ChatAudioFormat::Flac,
            "audio/opus" => q::ChatAudioFormat::Opus,
            _ => return Err(Error::invalid("unsupported Chat output audio format")),
        },
        _ => return Err(Error::invalid("Chat PCM output is 24 kHz mono")),
    })
}

pub(super) fn ir_format(format: q::ChatAudioFormat) -> AudioFormat {
    AudioFormat::Encoded {
        media_type: match format {
            q::ChatAudioFormat::Wav => "audio/wav",
            q::ChatAudioFormat::Mp3 => "audio/mpeg",
            q::ChatAudioFormat::Aac => "audio/aac",
            q::ChatAudioFormat::Flac => "audio/flac",
            q::ChatAudioFormat::Opus => "audio/opus",
            q::ChatAudioFormat::Pcm16 => return AudioFormat::pcm(24000, 1),
        }
        .into(),
    }
}

pub(crate) fn output_format(request: &GenerateRequest) -> Result<Option<AudioFormat>> {
    if let Some(audio) = request
        .provider_options
        .openai
        .chat
        .audio
        .as_ref()
        .and_then(Option::as_ref)
    {
        return Ok(Some(ir_format(audio.format)));
    }
    if let Some(speech) = &request.settings.speech {
        return Ok(Some(speech.format.clone()));
    }
    Ok(request
        .provider_options
        .openai
        .chat
        .audio
        .as_ref()
        .and_then(Option::as_ref)
        .map(|audio| ir_format(audio.format)))
}

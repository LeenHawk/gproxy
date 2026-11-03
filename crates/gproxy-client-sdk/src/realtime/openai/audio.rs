use super::*;
pub(in crate::realtime) fn format(value: &AudioFormat) -> Result<o::RealtimeAudioFormats> {
    use o::RealtimeAudioFormats as F;
    Ok(match value {
        AudioFormat::Pcm {
            media_type,
            sample_rate: 24000,
            channels: 1,
        } if media_type == "audio/pcm" || media_type == "audio/l16" => {
            F::RealtimeAudioFormatsAudioPCM(
                o::RealtimeAudioFormatsAudioPCM::builder()
                    .type_(Some(o::RealtimeAudioFormatsAudioPCMType::AudioPcm))
                    .rate(Some(o::RealtimeAudioFormatsAudioPCMRate::Value24000))
                    .build(),
            )
        }
        AudioFormat::Encoded { media_type } if media_type == "audio/pcmu" => {
            F::RealtimeAudioFormatsAudioPCMU(
                o::RealtimeAudioFormatsAudioPCMU::builder()
                    .type_(Some(o::RealtimeAudioFormatsAudioPCMUType::AudioPcmu))
                    .build(),
            )
        }
        AudioFormat::Encoded { media_type } if media_type == "audio/pcma" => {
            F::RealtimeAudioFormatsAudioPCMA(
                o::RealtimeAudioFormatsAudioPCMA::builder()
                    .type_(Some(o::RealtimeAudioFormatsAudioPCMAType::AudioPcma))
                    .build(),
            )
        }
        _ => {
            return Err(Error::invalid(
                "OpenAI Realtime accepts 24 kHz mono PCM, audio/pcmu or audio/pcma; no transcoding is performed",
            ));
        }
    })
}
pub(in crate::realtime) fn format_from_wire(value: &o::RealtimeAudioFormats) -> AudioFormat {
    match value {
        o::RealtimeAudioFormats::RealtimeAudioFormatsAudioPCM(_) => AudioFormat::pcm(24000, 1),
        o::RealtimeAudioFormats::RealtimeAudioFormatsAudioPCMU(_) => AudioFormat::Encoded {
            media_type: "audio/pcmu".into(),
        },
        o::RealtimeAudioFormats::RealtimeAudioFormatsAudioPCMA(_) => AudioFormat::Encoded {
            media_type: "audio/pcma".into(),
        },
    }
}
pub(in crate::realtime) fn turn(
    value: &TurnDetection,
    warnings: &mut Vec<Warning>,
) -> Option<Option<o::RealtimeAudioInputTurnDetection>> {
    use o::RealtimeAudioInputTurnDetection as T;
    match value {
        TurnDetection::Manual => Some(None),
        TurnDetection::ServerVad => Some(Some(T::RealtimeAudioInputTurnDetectionServerVad(
            o::RealtimeAudioInputTurnDetectionServerVad::builder(
                o::RealtimeAudioInputTurnDetectionServerVadType::ServerVad,
            )
            .build(),
        ))),
        TurnDetection::SemanticVad => Some(Some(T::RealtimeAudioInputTurnDetectionSemanticVad(
            o::RealtimeAudioInputTurnDetectionSemanticVad::builder(
                o::RealtimeAudioInputTurnDetectionSemanticVadType::SemanticVad,
            )
            .build(),
        ))),
        TurnDetection::Unknown(_) => {
            warn(warnings, "turn_detection", "Unknown turn detection omitted");
            None
        }
    }
}

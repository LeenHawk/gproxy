use super::*;
use crate::media_support::{self as shared, io};
use crate::provider_metadata::{MediaMetadata, PartMetadata, audio_metadata, part_fields};
use base64::Engine;
use protocol::gemini as g;
pub(super) struct State {
    pub result: SpeechResult,
    pub output: Option<Audio>,
    pub complete: bool,
    pub audio_seen: bool,
    stream_format: Option<AudioFormat>,
    pub part_metadata: Vec<protocol::Rest>,
}
impl State {
    pub fn new(warnings: Vec<Warning>) -> Self {
        Self {
            result: SpeechResult {
                audio: None,
                finish_reason: io::interrupted(),
                usage: OperationUsage::default(),
                warnings,
                response: ResponseMetadata::default(),
                provider_metadata: ProviderMetadata::default(),
            },
            output: None,
            complete: false,
            audio_seen: false,
            stream_format: None,
            part_metadata: Vec::new(),
        }
    }
    pub fn audio(
        &mut self,
        source: Asset,
        format: AudioFormat,
        meta: protocol::Rest,
        vendor: Vendor,
        emit: bool,
    ) -> Result<Option<ContentBlock<Audio>>> {
        self.audio_seen = true;
        if emit {
            if self.stream_format.as_ref().is_some_and(|f| *f != format) {
                return Err(Error::invalid("speech format changed mid-stream"));
            }
            self.stream_format = Some(format.clone());
            return Ok(Some(ContentBlock {
                content: Audio {
                    source,
                    format,
                    transcript: None,
                },
                cache: None,
                provider_metadata: [(vendor, meta)].into(),
            }));
        }
        if let Some(output) = &mut self.output {
            if output.format != format {
                return Err(Error::invalid(
                    "audio output changed format mid-stream; no transcoding is performed",
                ));
            }
            match (&mut output.source, source) {
                (Asset::Inline { bytes: current }, Asset::Inline { bytes }) => {
                    current.extend(bytes)
                }
                _ => {
                    return Err(Error::invalid(
                        "multiple URL/mixed speech fragments cannot be combined without fetching or transcoding",
                    ));
                }
            }
        } else {
            self.output = Some(Audio {
                source,
                format,
                transcript: None,
            });
        }
        self.part_metadata.push(meta);
        Ok(None)
    }
    pub fn chunk(
        &mut self,
        mut response: g::GenerateContentResponseBody,
        emit: bool,
    ) -> Result<Vec<ContentBlock<Audio>>> {
        shared::gemini::candidate_warning(&response, &mut self.result.warnings);
        self.result
            .response
            .merge(shared::gemini::response(&response));
        if response.usage_metadata.is_some() {
            self.result
                .usage
                .observe(shared::gemini::usage(response.usage_metadata.as_ref())?);
        }
        shared::gemini::observe_extras(
            self.result
                .provider_metadata
                .entry(Vendor::Gemini)
                .or_default(),
            &response,
        )?;
        let blocked = shared::gemini::blocked(&response);
        let mut chunks = Vec::new();
        let mut reason = None;
        if let Some(candidate) = response.candidates.take().into_iter().flatten().next() {
            reason = candidate.finish_reason;
            for mut part in candidate
                .content
                .into_iter()
                .flat_map(|c| c.parts.into_iter().flatten())
            {
                let mut meta = PartMetadata::default();
                let (source, format) = if let Some(blob) = part.inline_data.take() {
                    if !blob.mime_type.starts_with("audio/") {
                        return Err(Error::invalid(
                            "speech output contains non-audio binary data",
                        ));
                    }
                    let format = shared::audio::parse_format(&blob.mime_type)?;
                    meta.inline_data = Some(MediaMetadata {
                        mime_type: if matches!(format, AudioFormat::Pcm { .. })
                            && shared::audio::pcm_mime(&format)? != blob.mime_type
                        {
                            Some(blob.mime_type.clone())
                        } else {
                            None
                        },
                        display_name: blob.display_name,
                        rest: blob.rest,
                    });
                    (
                        Asset::Inline {
                            bytes: base64::engine::general_purpose::STANDARD
                                .decode(blob.data)
                                .map_err(Error::invalid)?,
                        },
                        format,
                    )
                } else if let Some(file) = part.file_data.take() {
                    let mime = file.mime_type.as_deref().ok_or_else(|| {
                        Error::invalid("speech output URL has no reported MIME type")
                    })?;
                    if !mime.starts_with("audio/") {
                        return Err(Error::invalid(
                            "speech output URL has a non-audio MIME type",
                        ));
                    }
                    let format = shared::audio::parse_format(mime)?;
                    meta.file_data = Some(MediaMetadata {
                        display_name: file.display_name,
                        rest: file.rest,
                        ..Default::default()
                    });
                    (Asset::Url { url: file.file_uri }, format)
                } else {
                    continue;
                };
                let transcript = part.audio_transcription.take().map(|value| {
                    let text = value.text.clone();
                    meta.audio_transcription = Some(audio_metadata(value));
                    text
                });
                meta.fields = part_fields(part);
                let mut chunk = self.audio(
                    source,
                    format,
                    shared::native_metadata(meta)?,
                    Vendor::Gemini,
                    emit,
                )?;
                if let Some(text) = transcript {
                    if let Some(chunk) = &mut chunk {
                        chunk.content.transcript = Some(text);
                    } else if let Some(audio) = &mut self.output {
                        audio
                            .transcript
                            .get_or_insert_with(String::new)
                            .push_str(&text);
                    }
                }
                chunks.extend(chunk);
            }
        }
        if blocked || reason.is_some() {
            self.complete = true;
            self.result.finish_reason =
                shared::gemini::finish(reason, response.prompt_feedback.as_ref())?;
        }
        if self.result.finish_reason.unified == FinishReasonKind::Stop && !self.audio_seen {
            return Err(Error::invalid("Gemini speech completed without audio"));
        }
        Ok(chunks)
    }
    pub fn finish(&mut self, vendor: Vendor) -> SpeechResult {
        let audio = self.output.take().map(|audio| {
            let single: std::result::Result<[protocol::Rest; 1], _> =
                std::mem::take(&mut self.part_metadata).try_into();
            let metadata = match single {
                Ok([only]) => only,
                Err(parts) => {
                    let mut extra = protocol::Rest::new();
                    extra.insert(
                        "speech_parts".into(),
                        serde_json::Value::Array(
                            parts.into_iter().map(serde_json::Value::Object).collect(),
                        ),
                    );
                    extra
                }
            };
            ContentBlock {
                content: audio,
                cache: None,
                provider_metadata: [(vendor, metadata)].into(),
            }
        });
        SpeechResult {
            audio,
            finish_reason: std::mem::replace(&mut self.result.finish_reason, io::interrupted()),
            usage: std::mem::take(&mut self.result.usage),
            warnings: std::mem::take(&mut self.result.warnings),
            response: std::mem::take(&mut self.result.response),
            provider_metadata: std::mem::take(&mut self.result.provider_metadata),
        }
    }
}

use super::*;
use crate::media_support::{
    self as shared,
    io::{self, Head, Input},
};
use base64::Engine;
use futures_util::StreamExt;
use protocol::openai::audio as o;
pub type SpeechStream = MediaStream<SpeechEvent>;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum SpeechEvent {
    Start {
        warnings: Vec<Warning>,
    },
    AudioChunk {
        audio: ContentBlock<Audio>,
    },
    Finish {
        finish_reason: FinishReason,
        usage: OperationUsage,
        warnings: Vec<Warning>,
        response: ResponseMetadata,
        provider_metadata: ProviderMetadata,
    },
    Error {
        error: Error,
    },
    #[serde(untagged)]
    Unknown(serde_json::Value),
}
impl MediaStream<SpeechEvent> {
    /// Only aggregation retains the complete audio; ordinary streaming does not.
    pub async fn aggregate(mut self) -> Result<SpeechResult> {
        let mut collected = state::State::new(Vec::new());
        let mut vendor = Vendor::Openai;
        while let Some(event) = self.next().await {
            match event {
                SpeechEvent::AudioChunk { audio } => {
                    vendor = if audio.provider_metadata.contains_key(&Vendor::Gemini) {
                        Vendor::Gemini
                    } else {
                        Vendor::Openai
                    };
                    let metadata = audio
                        .provider_metadata
                        .get(&vendor)
                        .cloned()
                        .unwrap_or_default();
                    collected.audio(
                        audio.content.source,
                        audio.content.format,
                        metadata,
                        vendor.clone(),
                        false,
                    )?;
                    if let (Some(text), Some(output)) =
                        (audio.content.transcript, collected.output.as_mut())
                    {
                        output
                            .transcript
                            .get_or_insert_with(String::new)
                            .push_str(&text);
                    }
                }
                SpeechEvent::Finish {
                    finish_reason,
                    usage,
                    warnings,
                    response,
                    provider_metadata,
                } => {
                    collected.result.finish_reason = finish_reason;
                    collected.result.usage = usage;
                    collected.result.warnings = warnings;
                    collected.result.response = response;
                    collected.result.provider_metadata = provider_metadata;
                    return Ok(collected.finish(vendor));
                }
                SpeechEvent::Error { error } => return Err(error),
                _ => {}
            }
        }
        Err(Error::invalid("speech stream has no finish event"))
    }
}
impl<B: Backend> Client<B> {
    pub async fn stream_speech(&self, request: &SpeechRequest) -> Result<SpeechStream> {
        request.check()?;
        let (mut wire, key, mut warnings, kind) = match request.model.dialect {
            SpeechDialect::Openai => {
                let rendered = openai::render(request, true)?;
                let (_, format) = openai::format(&request.format, &mut Vec::new());
                (
                    crate::client::wire_json(
                        http::Method::POST,
                        "/v1/audio/speech".into(),
                        &rendered.request,
                    )?,
                    OperationKey {
                        operation: protocol::Operation::CreateSpeech,
                        dialect: protocol::Dialect::OpenAi,
                    },
                    rendered.warnings,
                    Kind::Openai { format },
                )
            }
            SpeechDialect::Gemini => {
                let rendered = gemini::render(request)?;
                (
                    shared::gemini::wire(&request.model.id, &rendered.request, true)?,
                    OperationKey {
                        operation: protocol::Operation::StreamGenerateContent,
                        dialect: protocol::Dialect::Gemini,
                    },
                    rendered.warnings,
                    Kind::Gemini,
                )
            }
            SpeechDialect::Unknown(_) => return Err(Error::invalid("unsupported speech dialect")),
        };
        wire.headers.insert(
            http::header::ACCEPT,
            http::HeaderValue::from_static("text/event-stream"),
        );
        let response = self.send(key, Some(&request.model.id), wire).await?;
        if io::is_json(&response.headers) {
            shared::compatibility(
                &mut warnings,
                "stream",
                "The upstream returned completed JSON instead of SSE",
            );
            return io::read(response, self.limits, |body, _| {
                let native = io::parse(std::str::from_utf8(body).map_err(Error::invalid)?)?;
                if !matches!(kind, Kind::Gemini) {
                    return Err(Error::invalid(
                        "OpenAI speech returned JSON instead of audio events",
                    ));
                }
                let mut state = state::State::new(warnings.clone());
                let chunks = state.chunk(native, true)?;
                let mut events = vec![SpeechEvent::Start { warnings }];
                events.extend(
                    chunks
                        .into_iter()
                        .map(|audio| SpeechEvent::AudioChunk { audio }),
                );
                events.push(finish_event(&mut state));
                Ok(MediaStream::ready(events))
            })
            .await;
        }
        let mut decoder = Decoder {
            state: state::State::new(warnings.clone()),
            kind,
        };
        Ok(io::sse(
            response,
            self.limits,
            vec![SpeechEvent::Start { warnings }],
            Box::new(move |input, head| decoder.step(input, head)),
        ))
    }
}
enum Kind {
    Openai { format: AudioFormat },
    Gemini,
}
struct Decoder {
    state: state::State,
    kind: Kind,
}
fn finish_event(state: &mut state::State) -> SpeechEvent {
    SpeechEvent::Finish {
        finish_reason: std::mem::replace(&mut state.result.finish_reason, io::interrupted()),
        usage: std::mem::take(&mut state.result.usage),
        warnings: std::mem::take(&mut state.result.warnings),
        response: std::mem::take(&mut state.result.response),
        provider_metadata: std::mem::take(&mut state.result.provider_metadata),
    }
}
impl Decoder {
    fn step(&mut self, input: Input, head: &Head) -> (Vec<SpeechEvent>, bool) {
        let (result, done) = match input {
            Input::Data(data) => {
                let result = self
                    .push(&data, head)
                    .map_err(|e| head.error(e.with_body(data.as_bytes())));
                let done = matches!(self.kind, Kind::Openai { .. }) && self.state.complete;
                (result, done)
            }
            Input::Fault(error) => (Err(error), true),
            Input::End => (Ok(Vec::new()), true),
        };
        match result {
            Ok(mut events) => {
                if done {
                    events.push(finish_event(&mut self.state));
                }
                (events, done)
            }
            Err(error) => {
                self.state.result.finish_reason = if matches!(
                    error.kind,
                    ErrorKind::InvalidData | ErrorKind::Network | ErrorKind::Timeout
                ) {
                    io::interrupted()
                } else {
                    FinishReason::new(FinishReasonKind::Error, None)
                };
                (
                    vec![SpeechEvent::Error { error }, finish_event(&mut self.state)],
                    true,
                )
            }
        }
    }
    fn push(&mut self, data: &str, head: &Head) -> Result<Vec<SpeechEvent>> {
        let Kind::Openai { format } = &self.kind else {
            return self.state.chunk(io::parse(data)?, true).map(|chunks| {
                chunks
                    .into_iter()
                    .map(|audio| SpeechEvent::AudioChunk { audio })
                    .collect()
            });
        };
        self.state.result.response.merge(head.metadata());
        match io::parse::<o::SpeechAudioEvent>(data)? {
            o::SpeechAudioEvent::Delta(value) => {
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(value.audio)
                    .map_err(Error::invalid)?;
                self.state
                    .audio(
                        Asset::Inline { bytes },
                        format.clone(),
                        shared::metadata(value.rest)?,
                        Vendor::Openai,
                        true,
                    )
                    .map(|chunk| {
                        chunk
                            .into_iter()
                            .map(|audio| SpeechEvent::AudioChunk { audio })
                            .collect()
                    })
            }
            o::SpeechAudioEvent::Done(value) => {
                if !self.state.audio_seen {
                    return Err(Error::invalid("OpenAI speech completed without audio"));
                }
                self.state.result.usage = openai::usage(&value.usage)?;
                self.state.result.finish_reason = io::stop();
                self.state.complete = true;
                self.state.result.provider_metadata =
                    [(Vendor::Openai, shared::metadata(value.rest)?)].into();
                Ok(Vec::new())
            }
        }
    }
}

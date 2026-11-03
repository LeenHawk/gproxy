use super::*;
use crate::media_support::{
    self as shared,
    io::{self, Head, Input},
};
use futures_util::StreamExt;
use protocol::openai::audio as o;
pub type TranscriptionStream = MediaStream<TranscriptionEvent>;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum TranscriptionEvent {
    Start {
        warnings: Vec<Warning>,
    },
    Delta {
        text: String,
    },
    Segment {
        segment: TranscriptSegment,
    },
    Finish {
        result: Box<TranscriptionResult>,
    },
    Error {
        error: Error,
    },
    #[serde(untagged)]
    Unknown(serde_json::Value),
}
impl MediaStream<TranscriptionEvent> {
    pub async fn aggregate(mut self) -> Result<TranscriptionResult> {
        while let Some(event) = self.next().await {
            match event {
                TranscriptionEvent::Finish { result } => return Ok(*result),
                TranscriptionEvent::Error { error } => return Err(error),
                _ => {}
            }
        }
        Err(Error::invalid("transcription stream has no finish event"))
    }
}
impl<B: Backend> Client<B> {
    pub async fn stream_transcription(
        &self,
        request: &TranscriptionRequest,
    ) -> Result<TranscriptionStream> {
        self.transcription_stream(request, Vec::new()).await
    }
    pub async fn stream_transcription_prepared(
        &self,
        prepared: PreparedTranscriptionRequest,
    ) -> Result<TranscriptionStream> {
        self.transcription_stream(&prepared.request, prepared.warnings)
            .await
    }
    async fn transcription_stream(
        &self,
        request: &TranscriptionRequest,
        mut warnings: Vec<Warning>,
    ) -> Result<TranscriptionStream> {
        request.target()?;
        let (wire, key) = match request.model.dialect {
            TranscriptionDialect::Openai => {
                let rendered = openai::render(request, true)?;
                if rendered.request.response_format
                    == Some(o::TranscriptionResponseFormat::VerboseJson)
                {
                    let mut result = self.transcribe_audio(request).await?;
                    warnings.extend(rendered.warnings);
                    warnings.extend(result.warnings);
                    result.warnings = warnings.clone();
                    return Ok(ready(result, warnings));
                }
                warnings.extend(rendered.warnings);
                (
                    shared::multipart::transcription(
                        rendered.request,
                        request.provider_options.openai.rest.clone(),
                        self.limits,
                    )?,
                    OperationKey {
                        operation: protocol::Operation::CreateTranscription,
                        dialect: protocol::Dialect::OpenAi,
                    },
                )
            }
            TranscriptionDialect::Gemini => {
                let rendered = gemini::render(request)?;
                warnings.extend(rendered.warnings);
                (
                    shared::gemini::wire(&request.model.id, &rendered.request, true)?,
                    OperationKey {
                        operation: protocol::Operation::StreamGenerateContent,
                        dialect: protocol::Dialect::Gemini,
                    },
                )
            }
            TranscriptionDialect::Unknown(_) => {
                return Err(Error::invalid("unsupported transcription dialect"));
            }
        };
        let response = self.send(key, Some(&request.model.id), wire).await?;
        if io::is_json(&response.headers) {
            shared::compatibility(
                &mut warnings,
                "stream",
                "The upstream returned completed JSON instead of SSE",
            );
            let mut result = io::read(response, self.limits, |body, head| {
                let mut result = match request.model.dialect {
                    TranscriptionDialect::Openai => openai::parse(
                        std::str::from_utf8(body).map_err(Error::invalid)?,
                        openai::response_format(request),
                    )?,
                    TranscriptionDialect::Gemini => gemini::parse(io::parse(
                        std::str::from_utf8(body).map_err(Error::invalid)?,
                    )?)?,
                    TranscriptionDialect::Unknown(_) => {
                        return Err(Error::invalid("unsupported transcription dialect"));
                    }
                };
                let mut info = head.metadata();
                info.merge(result.response);
                result.response = info;
                Ok(result)
            })
            .await?;
            warnings.extend(result.warnings);
            result.warnings = warnings.clone();
            return Ok(ready(result, warnings));
        }
        let mut state = State {
            result: TranscriptionResult::empty(warnings.clone()),
            gemini: request.model.dialect == TranscriptionDialect::Gemini,
            complete: false,
        };
        Ok(io::sse(
            response,
            self.limits,
            vec![TranscriptionEvent::Start { warnings }],
            Box::new(move |input, head| state.step(input, head)),
        ))
    }
}
fn ready(result: TranscriptionResult, warnings: Vec<Warning>) -> TranscriptionStream {
    let mut events = vec![TranscriptionEvent::Start { warnings }];
    for segment in &result.segments {
        events.push(TranscriptionEvent::Segment {
            segment: segment.clone(),
        });
    }
    if !result.text.is_empty() {
        events.push(TranscriptionEvent::Delta {
            text: result.text.clone(),
        });
    }
    // The final result retains the full reported segment/word graph.
    events.push(TranscriptionEvent::Finish {
        result: Box::new(result),
    });
    MediaStream::ready(events)
}
struct State {
    result: TranscriptionResult,
    gemini: bool,
    complete: bool,
}
impl State {
    fn take(&mut self) -> TranscriptionResult {
        std::mem::replace(&mut self.result, TranscriptionResult::empty(Vec::new()))
    }
    fn step(&mut self, input: Input, head: &Head) -> (Vec<TranscriptionEvent>, bool) {
        match input {
            Input::Data(data) => match self.push(&data, head) {
                Ok(mut events) => {
                    let done = self.complete && !self.gemini;
                    if done {
                        events.push(TranscriptionEvent::Finish {
                            result: Box::new(self.take()),
                        });
                    }
                    (events, done)
                }
                Err(e) => self.fail(head.error(e.with_body(data.as_bytes()))),
            },
            Input::Fault(error) => self.fail(error),
            Input::End => {
                if !self.complete && self.result.text.is_empty() {
                    self.result.text = self
                        .result
                        .segments
                        .iter()
                        .map(|s| s.text.as_str())
                        .collect();
                }
                (
                    vec![TranscriptionEvent::Finish {
                        result: Box::new(self.take()),
                    }],
                    true,
                )
            }
        }
    }
    fn fail(&mut self, error: Error) -> (Vec<TranscriptionEvent>, bool) {
        self.result.finish_reason = if matches!(
            error.kind,
            ErrorKind::InvalidData | ErrorKind::Network | ErrorKind::Timeout
        ) {
            io::interrupted()
        } else {
            FinishReason::new(FinishReasonKind::Error, None)
        };
        (
            vec![
                TranscriptionEvent::Error { error },
                TranscriptionEvent::Finish {
                    result: Box::new(self.take()),
                },
            ],
            true,
        )
    }
    fn push(&mut self, data: &str, head: &Head) -> Result<Vec<TranscriptionEvent>> {
        if self.gemini {
            let segments = gemini::apply(&mut self.result, io::parse(data)?)?;
            if self.result.finish_reason.unified != FinishReasonKind::Interrupted {
                self.complete = true;
            }
            let mut events = Vec::new();
            for segment in segments {
                events.push(TranscriptionEvent::Delta {
                    text: segment.text.clone(),
                });
                events.push(TranscriptionEvent::Segment { segment });
            }
            return Ok(events);
        }
        self.result.response.merge(head.metadata());
        match io::parse::<o::TranscriptionStreamEvent>(data)? {
            o::TranscriptionStreamEvent::Segment(value) => {
                let segment = openai::diarized_segment(value)?;
                self.result.segments.push(segment.clone());
                Ok(vec![TranscriptionEvent::Segment { segment }])
            }
            o::TranscriptionStreamEvent::Text(o::TranscriptionTextStreamEvent::Delta(value)) => {
                self.result.text.push_str(&value.delta);
                Ok(vec![TranscriptionEvent::Delta { text: value.delta }])
            }
            o::TranscriptionStreamEvent::Text(o::TranscriptionTextStreamEvent::Done(value)) => {
                self.result.text = value.text;
                self.complete = true;
                self.result.finish_reason = io::stop();
                if let Some(usage) = value.usage {
                    self.result.usage = openai::token_usage(&usage)?;
                }
                #[derive(Serialize)]
                struct Extra {
                    languages: Option<Vec<o::TranscriptionLanguage>>,
                    logprobs: Option<Vec<o::TranscriptionLogprob>>,
                    #[serde(flatten)]
                    rest: protocol::Rest,
                }
                let languages = value
                    .languages
                    .as_ref()
                    .map(|ls| ls.iter().map(|l| l.code.clone()).collect::<Vec<_>>())
                    .unwrap_or_default();
                if languages.len() == 1 {
                    self.result.language = languages.first().cloned();
                }
                self.result.provider_metadata = [(
                    Vendor::Openai,
                    shared::metadata(Extra {
                        languages: value.languages,
                        logprobs: value.logprobs,
                        rest: value.rest,
                    })?,
                )]
                .into();
                Ok(Vec::new())
            }
        }
    }
}

use super::*;
use crate::dialects::StreamState as State;
/// Native event decoder, also useful when a host already owns SSE framing.
/// JSON is decoded into the native event enum exactly once.
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
pub struct StreamDecoder {
    state: State,
    started: bool,
    terminal: bool,
    initial_warnings: Vec<Warning>,
    seen_warnings: Vec<Warning>,
    response: ResponseMetadata,
    sources: Vec<Source>,
}
impl StreamDecoder {
    pub fn new(dialect: Dialect) -> Self {
        Self::with_audio_format(dialect, None)
    }
    pub fn with_audio_format(dialect: Dialect, _format: Option<AudioFormat>) -> Self {
        Self {
            started: false,
            terminal: false,
            initial_warnings: Vec::new(),
            seen_warnings: Vec::new(),
            response: ResponseMetadata::default(),
            sources: Vec::new(),
            state: State::new(dialect, _format),
        }
    }
    pub fn with_warnings(
        dialect: Dialect,
        format: Option<AudioFormat>,
        warnings: Vec<Warning>,
    ) -> Self {
        let mut decoder = Self::with_audio_format(dialect, format);
        decoder.initial_warnings = warnings;
        decoder
    }
    pub(super) fn start(&mut self) -> Vec<Event> {
        if self.started {
            Vec::new()
        } else {
            self.started = true;
            vec![Event::Start {
                warnings: std::mem::take(&mut self.initial_warnings),
            }]
        }
    }
    pub(super) fn observed_usage(&self) -> Usage {
        self.state.usage()
    }
    pub fn push_json(&mut self, data: &str) -> Result<Vec<Event>> {
        if self.terminal {
            return Ok(Vec::new());
        }
        let mut out = self.start();
        let events = match self.decode_json(data) {
            Ok(events) => events,
            Err(error)
                if error.body.is_some()
                    && !matches!(
                        error.kind,
                        crate::ErrorKind::InvalidData | crate::ErrorKind::Network
                    ) =>
            {
                vec![
                    Event::Error { error },
                    Event::Finish {
                        reason: FinishReason::new(FinishReasonKind::Error, None),
                        usage: self.observed_usage(),
                    },
                ]
            }
            Err(error) => return Err(error.with_body(data.as_bytes())),
        };
        for event in events {
            match event {
                Event::ResponseMetadata { metadata } => {
                    let before = self.response.clone();
                    self.response.merge(metadata);
                    if self.response != before {
                        out.push(Event::ResponseMetadata {
                            metadata: self.response.clone(),
                        });
                    }
                }
                Event::Source { source } => {
                    if !self.sources.contains(&source) {
                        self.sources.push(source.clone());
                        out.push(Event::Source { source });
                    }
                }
                Event::Warning { warning } => {
                    if !self.seen_warnings.contains(&warning) {
                        self.seen_warnings.push(warning.clone());
                        out.push(Event::Warning { warning });
                    }
                }
                Event::Error { error } => out.push(Event::Error {
                    error: error.with_body(data.as_bytes()),
                }),
                Event::Finish { reason, usage } => {
                    self.terminal = true;
                    out.push(Event::Finish { reason, usage });
                }
                event => out.push(event),
            }
        }
        Ok(out)
    }
    fn decode_json(&mut self, data: &str) -> Result<Vec<Event>> {
        self.state.push(data)
    }
    pub fn finish(mut self) -> Result<Vec<Event>> {
        let mut out = self.start();
        if self.terminal {
            return Ok(out);
        }
        out.extend(self.state.finish()?);
        Ok(out)
    }
}

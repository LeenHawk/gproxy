use super::*;
use crate::media_support::io;
use protocol::Operation;
impl<B: Backend> Client<B> {
    pub async fn synthesize(&self, request: &SpeechRequest) -> Result<SpeechResult> {
        request.check()?;
        match request.model.dialect {
            SpeechDialect::Openai => {
                let rendered = openai::render(request, false)?;
                let (_, effective) = openai::format(&request.format, &mut Vec::new());
                let wire = crate::client::wire_json(
                    http::Method::POST,
                    "/v1/audio/speech".into(),
                    &rendered.request,
                )?;
                let response = self
                    .send(
                        OperationKey {
                            operation: Operation::CreateSpeech,
                            dialect: protocol::Dialect::OpenAi,
                        },
                        Some(&request.model.id),
                        wire,
                    )
                    .await?;
                io::read(response, self.limits, |body, head| {
                    let format = if let Some(mime) = head
                        .headers
                        .get(http::header::CONTENT_TYPE)
                        .and_then(|h| h.to_str().ok())
                    {
                        if mime.starts_with("application/json") {
                            io::parse::<serde_json::Value>(
                                std::str::from_utf8(body).map_err(Error::invalid)?,
                            )?;
                            return Err(Error::invalid(
                                "speech endpoint returned JSON instead of audio",
                            ));
                        }
                        if mime.split(';').next() == Some("audio/pcm")
                            && matches!(effective, AudioFormat::Pcm { .. })
                        {
                            effective
                        } else if mime.starts_with("audio/") {
                            crate::media_support::audio::parse_format(mime)?
                        } else {
                            effective
                        }
                    } else {
                        effective
                    };
                    let mut state = state::State::new(rendered.warnings);
                    state.result.response = head.metadata();
                    if body.is_empty() {
                        return Err(Error::invalid("speech endpoint returned no audio"));
                    }
                    state.audio(
                        Asset::Inline {
                            bytes: body.to_vec(),
                        },
                        format,
                        protocol::Rest::new(),
                        Vendor::Openai,
                        false,
                    )?;
                    state.result.finish_reason = io::stop();
                    state.complete = true;
                    Ok(state.finish(Vendor::Openai))
                })
                .await
            }
            SpeechDialect::Gemini => {
                let rendered = gemini::render(request)?;
                let wire = crate::media_support::gemini::wire(
                    &request.model.id,
                    &rendered.request,
                    false,
                )?;
                let response = self
                    .send(
                        OperationKey {
                            operation: Operation::GenerateContent,
                            dialect: protocol::Dialect::Gemini,
                        },
                        Some(&request.model.id),
                        wire,
                    )
                    .await?;
                io::read(response, self.limits, |body, _| {
                    let mut state = state::State::new(rendered.warnings);
                    state.chunk(
                        io::parse(std::str::from_utf8(body).map_err(Error::invalid)?)?,
                        false,
                    )?;
                    Ok(state.finish(Vendor::Gemini))
                })
                .await
            }
            SpeechDialect::Unknown(_) => Err(Error::invalid("unsupported speech dialect")),
        }
    }
}

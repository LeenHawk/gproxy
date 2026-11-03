use super::*;
use crate::media_support::{self as shared, io};
impl<B: Backend> Client<B> {
    pub async fn transcribe_audio(
        &self,
        request: &TranscriptionRequest,
    ) -> Result<TranscriptionResult> {
        request.target()?;
        match request.model.dialect {
            TranscriptionDialect::Openai => {
                let rendered = openai::render(request, false)?;
                let format = openai::response_format(request);
                let wire = shared::multipart::transcription(
                    rendered.request,
                    request.provider_options.openai.rest.clone(),
                    self.limits,
                )?;
                let response = self
                    .send(
                        OperationKey {
                            operation: protocol::Operation::CreateTranscription,
                            dialect: protocol::Dialect::OpenAi,
                        },
                        Some(&request.model.id),
                        wire,
                    )
                    .await?;
                io::read(response, self.limits, |body, head| {
                    let mut result =
                        openai::parse(std::str::from_utf8(body).map_err(Error::invalid)?, format)?;
                    result.response.merge(head.metadata());
                    let mut warnings = rendered.warnings;
                    warnings.extend(result.warnings);
                    result.warnings = warnings;
                    Ok(result)
                })
                .await
            }
            TranscriptionDialect::Gemini => {
                let rendered = gemini::render(request)?;
                let wire = shared::gemini::wire(&request.model.id, &rendered.request, false)?;
                let response = self
                    .send(
                        OperationKey {
                            operation: protocol::Operation::GenerateContent,
                            dialect: protocol::Dialect::Gemini,
                        },
                        Some(&request.model.id),
                        wire,
                    )
                    .await?;
                io::read(response, self.limits, |body, _| {
                    let mut result = gemini::parse(io::parse(
                        std::str::from_utf8(body).map_err(Error::invalid)?,
                    )?)?;
                    let mut warnings = rendered.warnings;
                    warnings.extend(result.warnings);
                    result.warnings = warnings;
                    Ok(result)
                })
                .await
            }
            TranscriptionDialect::Unknown(_) => {
                Err(Error::invalid("unsupported transcription dialect"))
            }
        }
    }
    pub async fn transcribe_audio_prepared(
        &self,
        prepared: PreparedTranscriptionRequest,
    ) -> Result<TranscriptionResult> {
        let mut result = self.transcribe_audio(&prepared.request).await?;
        let mut warnings = prepared.warnings;
        warnings.extend(result.warnings);
        result.warnings = warnings;
        Ok(result)
    }
}

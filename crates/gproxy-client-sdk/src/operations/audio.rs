use super::Params;
use crate::*;
use gproxy_protocol::{Operation, codec::MultipartEncoder, connection::MultipartPart, openai};
use http::{HeaderMap, HeaderValue, Method};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[cfg(feature = "audio")]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TranscriptionParams {
    pub model: String,
    pub vendor: Vendor,
    pub bytes: Vec<u8>,
    pub filename: String,
    pub media_type: String,
    pub language: Option<String>,
    #[serde(default)]
    pub provider_options: ProviderOptions,
}

#[cfg(feature = "audio")]
impl<B: Backend> Client<B> {
    /// Audio bytes, text, or native SSE according to the typed native parameters.
    pub async fn speech(
        &self,
        request: &Params<openai::audio::CreateSpeechRequestBody>,
    ) -> Result<WireResponse<HttpBody>> {
        self.send(
            OperationKey {
                operation: Operation::CreateSpeech,
                dialect: gproxy_protocol::Dialect::OpenAi,
            },
            Some(&request.native.model),
            wire_json(
                Method::POST,
                "/v1/audio/speech".into(),
                &native_params!(request),
            )?,
        )
        .await
    }
    /// Returns the native response transport, since transcription can be JSON,
    /// text, verbose JSON, diarized JSON or SSE (selected in provider_options).
    pub async fn transcribe(
        &self,
        request: &TranscriptionParams,
    ) -> Result<WireResponse<HttpBody>> {
        let mut boundary_bytes = [0u8; 16];
        getrandom::fill(&mut boundary_bytes).map_err(Error::invalid)?;
        let boundary = format!("gproxy-{:032x}", u128::from_le_bytes(boundary_bytes));
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::CONTENT_DISPOSITION,
            HeaderValue::from_str(&format!(
                "form-data; name=\"file\"; filename=\"{}\"",
                request.filename.replace('\\', "\\\\").replace('"', "\\\"")
            ))
            .map_err(Error::invalid)?,
        );
        headers.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_str(&request.media_type).map_err(Error::invalid)?,
        );
        let mut parts = vec![MultipartPart {
            headers,
            body: HttpBody::Bytes(request.bytes.clone().into()),
        }];
        let mut fields = request.provider_options.remainder(&request.vendor)?.clone();
        fields.insert("model".into(), request.model.clone().into());
        if let Some(language) = &request.language {
            fields.insert("language".into(), language.clone().into());
        }
        for (name, value) in fields {
            let values = if let Value::Array(values) = value {
                values
            } else {
                vec![value]
            };
            for value in values {
                let mut headers = HeaderMap::new();
                headers.insert(
                    http::header::CONTENT_DISPOSITION,
                    HeaderValue::from_str(&format!(
                        "form-data; name=\"{}\"",
                        name.replace('"', "\\\"")
                    ))
                    .map_err(Error::invalid)?,
                );
                parts.push(MultipartPart {
                    headers,
                    body: HttpBody::Bytes(
                        value
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| value.to_string())
                            .into(),
                    ),
                });
            }
        }
        let encoder =
            MultipartEncoder::new(boundary.clone(), parts, self.limits).map_err(Error::invalid)?;
        let bytes = futures_util::stream::try_unfold(encoder, |mut encoder| async move {
            let chunk = encoder
                .next_chunk()
                .await
                .map_err(|e| Box::new(e) as gproxy_protocol::connection::TransportError)?;
            Ok(chunk.map(|chunk| (chunk, encoder)))
        });
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_str(&format!("multipart/form-data; boundary={boundary}"))
                .map_err(Error::invalid)?,
        );
        self.send(
            OperationKey {
                operation: Operation::CreateTranscription,
                dialect: gproxy_protocol::Dialect::OpenAi,
            },
            Some(&request.model),
            WireRequest {
                method: Method::POST,
                path: "/v1/audio/transcriptions".into(),
                query: None,
                headers,
                body: HttpBody::Stream(Box::pin(bytes)),
            },
        )
        .await
    }
}

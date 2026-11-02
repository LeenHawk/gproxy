//! Thin typed operation families. GPROXY handles native upstream adaptation.
// Each import/helper is used by an independently selectable operation family.
#![allow(unused_imports, dead_code)]
use crate::*;
use gproxy_protocol::{
    Operation, Rest,
    codec::MultipartEncoder,
    connection::{Bytes, MultipartPart},
    openai,
};
use http::{HeaderMap, HeaderValue, Method};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Native typed parameters plus options for the selected target vendor.
/// No universal media/embedding format is imposed on the response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Params<T> {
    pub native: T,
    pub vendor: Vendor,
    #[serde(default)]
    pub provider_options: ProviderOptions,
}
impl<T> Params<T> {
    pub fn new(native: T, vendor: Vendor) -> Self {
        Self {
            native,
            vendor,
            provider_options: ProviderOptions::new(),
        }
    }
}
fn params<T: Serialize>(params: &Params<T>) -> Result<Value> {
    let mut value: Rest = params
        .provider_options
        .get(&params.vendor)
        .cloned()
        .unwrap_or_default();
    let Value::Object(native) = serde_json::to_value(&params.native).map_err(Error::invalid)?
    else {
        return Err(Error::invalid(
            "native operation parameters must be an object",
        ));
    };
    value.extend(native);
    Ok(value.into())
}

// The operation's type and path are a single declaration. All of these use
// GPROXY's public OpenAI-shaped ingress; the target channel adapts upstream.
macro_rules! json_operation {
    ($feature:literal,$method:ident,$input:ty,$output:ty,$operation:ident,$path:literal) => {
        #[cfg(feature = $feature)]
        impl<B: Backend> Client<B> {
            pub async fn $method(&self, request: &Params<$input>) -> Result<$output> {
                let value = params(request)?;
                let model = value["model"].as_str();
                self.json(
                    OperationKey {
                        operation: Operation::$operation,
                        dialect: gproxy_protocol::Dialect::OpenAi,
                    },
                    model,
                    wire_json(Method::POST, $path.into(), &value)?,
                )
                .await
            }
        }
    };
}
json_operation!(
    "embeddings",
    embeddings,
    openai::embeddings::CreateEmbeddingRequestBody,
    openai::embeddings::CreateEmbeddingResponseBody,
    CreateEmbedding,
    "/v1/embeddings"
);
json_operation!(
    "rerank",
    rerank,
    openai::rerank::RerankRequestBody,
    openai::rerank::RerankResponseBody,
    Rerank,
    "/v1/rerank"
);
json_operation!(
    "images",
    create_image,
    openai::images::CreateImageRequestBody,
    openai::images::ImagesResponse,
    CreateImage,
    "/v1/images/generations"
);
json_operation!(
    "images",
    edit_image,
    openai::images::EditImageJsonBody,
    openai::images::ImagesResponse,
    EditImage,
    "/v1/images/edits"
);
json_operation!(
    "video",
    create_video,
    openai::video::NativeCreateVideoRequestBody,
    openai::video::NativeVideo,
    CreateVideo,
    "/v1/videos"
);

#[cfg(feature = "models")]
impl<B: Backend> Client<B> {
    /// Returns the normal permission-filtered local catalog. Its owner field
    /// is not a dialect contract; callers explicitly construct `Model`.
    pub async fn list_models(&self) -> Result<openai::models::ListModelsResponseBody> {
        self.json(
            OperationKey {
                operation: Operation::ListModels,
                dialect: gproxy_protocol::Dialect::OpenAi,
            },
            None,
            empty_request(Method::GET, "/v1/models".into()),
        )
        .await
    }
}

#[cfg(feature = "count-tokens")]
#[derive(Debug, Clone, PartialEq)]
pub enum CountTokensParams {
    #[cfg(feature = "claude")]
    Claude(Params<gproxy_protocol::claude::count_tokens::CountTokensRequestBody>),
    #[cfg(feature = "openai")]
    Openai(Params<openai::count_tokens::CountTokensRequestBody>),
    #[cfg(feature = "gemini")]
    Gemini {
        model: String,
        params: Box<Params<gproxy_protocol::gemini::count_tokens::CountTokensRequestBody>>,
    },
}
#[cfg(feature = "count-tokens")]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TokenCount {
    pub input_tokens: Option<u64>,
    pub provider_metadata: ProviderMetadata,
}
#[cfg(all(
    feature = "count-tokens",
    any(feature = "claude", feature = "openai", feature = "gemini")
))]
impl<B: Backend> Client<B> {
    pub async fn count_tokens(&self, request: &CountTokensParams) -> Result<TokenCount> {
        let (vendor, count, native) = match request {
            #[cfg(feature = "claude")]
            CountTokensParams::Claude(p) => {
                let mut wire = wire_json(
                    Method::POST,
                    "/v1/messages/count_tokens".into(),
                    &params(p)?,
                )?;
                wire.headers
                    .insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
                let body: protocol::claude::count_tokens::CountTokensResponseBody = self
                    .json(
                        OperationKey {
                            operation: Operation::CountTokens,
                            dialect: protocol::Dialect::Claude,
                        },
                        Some(&p.native.model),
                        wire,
                    )
                    .await?;
                (
                    Vendor::Claude,
                    Some(u64::try_from(body.input_tokens).map_err(Error::invalid)?),
                    serde_json::to_value(body).map_err(Error::invalid)?,
                )
            }
            #[cfg(feature = "openai")]
            CountTokensParams::Openai(p) => {
                let model = p
                    .native
                    .model
                    .as_ref()
                    .and_then(Option::as_ref)
                    .map(String::as_str);
                let wire = wire_json(
                    Method::POST,
                    "/v1/responses/input_tokens".into(),
                    &params(p)?,
                )?;
                let body: protocol::openai::count_tokens::CountTokensResponseBody = self
                    .json(
                        OperationKey {
                            operation: Operation::CountTokens,
                            dialect: protocol::Dialect::OpenAi,
                        },
                        model,
                        wire,
                    )
                    .await?;
                (
                    Vendor::Openai,
                    Some(u64::try_from(body.input_tokens).map_err(Error::invalid)?),
                    serde_json::to_value(body).map_err(Error::invalid)?,
                )
            }
            #[cfg(feature = "gemini")]
            CountTokensParams::Gemini { model, params: p } => {
                let wire = wire_json(
                    Method::POST,
                    format!("/v1beta/models/{}:countTokens", path_segment(model)),
                    &params(p)?,
                )?;
                let body: protocol::gemini::count_tokens::CountTokensResponseBody = self
                    .json(
                        OperationKey {
                            operation: Operation::CountTokens,
                            dialect: protocol::Dialect::Gemini,
                        },
                        Some(model),
                        wire,
                    )
                    .await?;
                (
                    Vendor::Gemini,
                    body.total_tokens
                        .map(u64::try_from)
                        .transpose()
                        .map_err(Error::invalid)?,
                    serde_json::to_value(body).map_err(Error::invalid)?,
                )
            }
        };
        Ok(TokenCount {
            input_tokens: count,
            provider_metadata: [(vendor, [("native".into(), native)].into_iter().collect())].into(),
        })
    }
}

#[cfg(feature = "video")]
impl<B: Backend> Client<B> {
    pub async fn retrieve_video(&self, id: &str) -> Result<openai::video::NativeVideo> {
        self.json(
            OperationKey {
                operation: Operation::RetrieveVideo,
                dialect: gproxy_protocol::Dialect::OpenAi,
            },
            None,
            empty_request(Method::GET, format!("/v1/videos/{}", path_segment(id))),
        )
        .await
    }
    pub async fn list_videos(
        &self,
        query: &openai::video::NativeListVideosQuery,
    ) -> Result<openai::video::NativeListVideosResponseBody> {
        let mut request = empty_request(Method::GET, "/v1/videos".into());
        request.query = Some(serde_urlencoded::to_string(query).map_err(Error::invalid)?);
        self.json(
            OperationKey {
                operation: Operation::ListVideos,
                dialect: gproxy_protocol::Dialect::OpenAi,
            },
            None,
            request,
        )
        .await
    }
    pub async fn delete_video(
        &self,
        id: &str,
    ) -> Result<openai::video::NativeDeleteVideoResponseBody> {
        self.json(
            OperationKey {
                operation: Operation::DeleteVideo,
                dialect: gproxy_protocol::Dialect::OpenAi,
            },
            None,
            empty_request(Method::DELETE, format!("/v1/videos/{}", path_segment(id))),
        )
        .await
    }
    pub async fn download_video(&self, id: &str) -> Result<WireResponse<HttpBody>> {
        self.send(
            OperationKey {
                operation: Operation::DownloadVideoContent,
                dialect: gproxy_protocol::Dialect::OpenAi,
            },
            None,
            empty_request(
                Method::GET,
                format!("/v1/videos/{}/content", path_segment(id)),
            ),
        )
        .await
    }
}

fn empty_request(method: Method, path: String) -> WireRequest<HttpBody> {
    WireRequest {
        method,
        path,
        query: None,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(Bytes::new()),
    }
}

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
            wire_json(Method::POST, "/v1/audio/speech".into(), &params(request)?)?,
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
        let mut fields = request
            .provider_options
            .get(&request.vendor)
            .cloned()
            .unwrap_or_default();
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

use gproxy_protocol::{
    Dialect, Operation, OperationKey,
    wire::{claude as c, gemini as g, openai as o},
};
use serde_json::{Map, Value, json};

use super::schemas::{Schemas, alternatives};

pub fn error_response(s: &mut Schemas) -> Value {
    let openai = s.output::<o::responses::websocket::ErrorDetail>();
    let claude = s.output::<c::stream::ErrorEvent>();
    let google = s.output::<g::generate_content::GenerateContentError>();
    let gateway = crate::error::schema(s);
    json!({
        "description":"Gateway or upstream error",
        "content":{
            "application/json":{"schema":alternatives(vec![gateway,
                json!({"type":"object","required":["error"],"properties":{"error":openai}}),
                json!({"allOf":[claude,{"type":"object","required":["type"],"properties":{"type":{"const":"error"}}}]}),
                json!({"type":"object","required":["error"],"properties":{"error":google}}),
            ])},
            "text/plain":{"schema":{"type":"string"}},
        },
    })
}

#[derive(Default)]
pub struct Payloads {
    pub request: Map<String, Value>,
    pub response: Map<String, Value>,
    pub parameters: Vec<Value>,
    pub websocket: Option<Value>,
}

impl Payloads {
    fn request(mut self, media: &str, schema: Value) -> Self {
        self.request.insert(media.into(), json!({"schema": schema}));
        self
    }
    fn response(mut self, media: &str, schema: Value) -> Self {
        self.response
            .insert(media.into(), json!({"schema": schema}));
        self
    }
    fn sse(mut self, event: Value) -> Self {
        // The HTTP body is SSE text. The extension describes each decoded
        // data event rather than pretending the entire stream is one JSON value.
        self.response.insert(
            "text/event-stream".into(),
            json!({
                "schema": {"type":"string"}, "x-event-schema": event,
            }),
        );
        self
    }
    fn query(mut self, parameters: Vec<Value>) -> Self {
        self.parameters = parameters;
        self
    }
}

/// Every public wire surface must select concrete Rust payload types here.
/// The exporter checks all OPERATION_SPECS, including body-flag streaming and
/// WebSocket messages that do not appear as distinct HTTP paths.
pub fn payloads(s: &mut Schemas, key: OperationKey, path: &str) -> Payloads {
    use Dialect::*;
    use Operation::*;
    let empty = Payloads::default;
    let binary = || json!({"type":"string", "format":"binary"});
    let text = || json!({"type":"string"});
    macro_rules! pair {
        ($input:ty, $output:ty) => {
            empty()
                .request("application/json", s.input::<$input>())
                .response("application/json", s.output::<$output>())
        };
    }
    macro_rules! output {
        ($output:ty) => {
            empty().response("application/json", s.output::<$output>())
        };
    }
    match (key.operation, key.dialect) {
        (ListModels, OpenAi) => output!(o::models::ListModelsResponseBody),
        (GetModel, OpenAi) => output!(o::models::Model),
        (ListModels, Claude) => output!(c::models::ListModelsResponseBody)
            .query(s.query::<c::models::ListModelsQuery>()),
        (GetModel, Claude) => output!(c::models::ModelInfo),
        (ListModels, Gemini) => output!(g::models::ListModelsResponseBody)
            .query(s.query::<g::models::ListModelsQuery>()),
        (GetModel, Gemini) => output!(g::models::Model),
        (CountTokens, OpenAi) => pair!(
            o::count_tokens::CountTokensRequestBody,
            o::count_tokens::CountTokensResponseBody
        ),
        (CountTokens, Claude) => pair!(
            c::count_tokens::CountTokensRequestBody,
            c::count_tokens::CountTokensResponseBody
        ),
        (CountTokens, Gemini) => pair!(
            g::count_tokens::CountTokensRequestBody,
            g::count_tokens::CountTokensResponseBody
        ),
        (GenerateContent, OpenAi) => pair!(
            o::responses::GenerateContentRequestBody,
            o::responses::GenerateContentResponseBody
        ),
        (GenerateContent, OpenAiChat) => pair!(
            o::chat::GenerateContentRequestBody,
            o::chat::GenerateContentResponseBody
        ),
        (GenerateContent, Claude) => pair!(
            c::generate_content::GenerateContentRequestBody,
            c::generate_content::GenerateContentResponseBody
        ),
        (GenerateContent, Gemini) => pair!(
            g::GenerateContentRequestBody,
            g::GenerateContentResponseBody
        ),
        (StreamGenerateContent, OpenAi) => empty()
            .request(
                "application/json",
                s.input::<o::responses::GenerateContentRequestBody>(),
            )
            .sse(s.output::<o::responses::stream::StreamEvent>()),
        (StreamGenerateContent, OpenAiChat) => empty()
            .request(
                "application/json",
                s.input::<o::chat::GenerateContentRequestBody>(),
            )
            .sse(s.output::<o::chat::stream::ChatCompletionChunk>()),
        (StreamGenerateContent, Claude) => empty()
            .request(
                "application/json",
                s.input::<c::generate_content::GenerateContentRequestBody>(),
            )
            .sse(s.output::<c::stream::StreamEvent>()),
        (StreamGenerateContent, Gemini) => {
            let event = s.output::<g::stream::StreamChunk>();
            empty()
                .request(
                    "application/json",
                    s.input::<g::GenerateContentRequestBody>(),
                )
                .sse(event.clone())
                .response("application/json", json!({"type":"array","items":event}))
                .query(vec![json!({"in":"query","name":"alt","required":false,"schema":{"type":"string","enum":["sse","json"]}})])
        }
        (CreateModeration, OpenAi) => pair!(
            o::moderation::CreateModerationRequestBody,
            o::moderation::CreateModerationResponseBody
        ),
        (CreateDecision, OpenAi) => pair!(
            o::decisions::CreateDecisionRequestBody,
            o::decisions::CreateDecisionResponseBody
        ),
        (GuardianReview | GuardianClassify, OpenAi) => empty()
            .request(
                "application/json",
                s.input::<o::guardian::GuardianRequestBody>(),
            )
            .sse(s.output::<o::guardian::GuardianEvent>()),
        (CompactContent, OpenAi) => pair!(
            o::compact::CompactRequestBody,
            o::compact::CompactResponseBody
        ),
        (SummarizeMemory, OpenAi) => pair!(
            o::memory::MemorySummarizeRequestBody,
            o::memory::MemorySummarizeResponseBody
        ),
        (CreateConversation, OpenAi) => pair!(
            o::conversation::CreateConversationRequestBody,
            o::conversation::ConversationResponseBody
        ),
        (WebSearch, OpenAi) => pair!(
            o::web_search::WebSearchRequestBody,
            o::web_search::WebSearchResponseBody
        ),
        (Rerank, OpenAi) => pair!(o::rerank::RerankRequestBody, o::rerank::RerankResponseBody),
        (CreateEmbedding, OpenAi) => pair!(
            o::embeddings::CreateEmbeddingRequestBody,
            o::embeddings::CreateEmbeddingResponseBody
        ),
        (CreateEmbedding, Gemini) => pair!(
            g::embeddings::EmbedContentRequestBody,
            g::embeddings::EmbedContentResponseBody
        ),
        (BatchCreateEmbedding, Gemini) => pair!(
            g::embeddings::BatchEmbedContentsRequestBody,
            g::embeddings::BatchEmbedContentsResponseBody
        ),
        (CreateImage, OpenAi) => {
            pair!(o::images::CreateImageRequestBody, o::images::ImagesResponse)
                .sse(s.output::<o::images::ImageGenerationStreamEvent>())
        }
        (EditImage, OpenAi) => pair!(o::images::EditImageJsonBody, o::images::ImagesResponse)
            .request(
                "multipart/form-data",
                s.input::<o::images::EditImageMultipartForm>(),
            )
            .sse(s.output::<o::images::ImageEditStreamEvent>()),
        (CreateSpeech, OpenAi) => empty()
            .request(
                "application/json",
                s.input::<o::audio::CreateSpeechRequestBody>(),
            )
            .response("audio/*", binary())
            .response("application/octet-stream", binary())
            .sse(s.output::<o::audio::SpeechAudioEvent>()),
        (CreateTranscription, OpenAi) => {
            let response = alternatives(vec![
                s.output::<o::audio::TranscriptionJson>(),
                s.output::<o::audio::TranscriptionVerbose>(),
                s.output::<o::audio::TranscriptionDiarized>(),
            ]);
            empty()
                .request(
                    "multipart/form-data",
                    s.input::<o::audio::CreateTranscriptionMultipartForm>(),
                )
                .response("application/json", response)
                .response("text/plain", text())
                .response("text/vtt", text())
                .response("application/x-subrip", text())
                .sse(s.output::<o::audio::TranscriptionStreamEvent>())
        }
        (CreateTranslation, OpenAi) => {
            let response = alternatives(vec![
                s.output::<o::audio::TranslationJson>(),
                s.output::<o::audio::TranslationVerbose>(),
            ]);
            empty()
                .request(
                    "multipart/form-data",
                    s.input::<o::audio::CreateTranslationMultipartForm>(),
                )
                .response("application/json", response)
                .response("text/plain", text())
                .response("text/vtt", text())
                .response("application/x-subrip", text())
        }
        (CreateFile, OpenAi) => output!(o::files::FileObject)
            .request("multipart/form-data", s.input::<o::files::UploadFileForm>()),
        (CreateFile, Claude) => output!(c::files::FileMetadata)
            .request("multipart/form-data", s.input::<c::files::UploadFileForm>()),
        (CreateFile, Gemini) => output!(g::files::UploadFileResponseBody)
            .request("multipart/related", s.input::<g::files::UploadFileForm>())
            .request(
                "application/json",
                s.input::<g::files::UploadFileMetadata>(),
            )
            .request("application/octet-stream", binary()),
        (ListFiles, OpenAi) => {
            output!(o::files::ListFilesResponseBody).query(s.query::<o::files::ListFilesQuery>())
        }
        (ListFiles, Claude) => {
            output!(c::files::ListFilesResponseBody).query(s.query::<c::files::ListFilesQuery>())
        }
        (ListFiles, Gemini) => {
            output!(g::files::ListFilesResponseBody).query(s.query::<g::files::ListFilesQuery>())
        }
        (RetrieveFile, OpenAi) => output!(o::files::FileObject),
        (RetrieveFile, Claude) => output!(c::files::FileMetadata),
        (RetrieveFile, Gemini) => output!(g::files::File),
        (RetrieveFileContent, OpenAi | Claude | Gemini) => {
            empty().response("application/octet-stream", binary())
        }
        (DeleteFile, OpenAi) => output!(o::files::DeleteFileResponseBody),
        (DeleteFile, Claude) => output!(c::files::DeleteFileResponseBody),
        (DeleteFile, Gemini) => output!(g::files::DeleteFileResponseBody),
        (CreateVideo, OpenAi) => pair!(
            o::video::NativeCreateVideoRequestBody,
            o::video::NativeVideo
        )
        .request(
            "multipart/form-data",
            s.input::<o::video::NativeCreateVideoMultipartForm>(),
        ),
        (RetrieveVideo, OpenAi) => output!(o::video::NativeVideo),
        (ListVideos, OpenAi) => output!(o::video::NativeListVideosResponseBody)
            .query(s.query::<o::video::NativeListVideosQuery>()),
        (DeleteVideo, OpenAi) => output!(o::video::NativeDeleteVideoResponseBody),
        (DownloadVideoContent, OpenAi) => empty()
            .response("application/octet-stream", binary())
            .query(s.query::<o::video::NativeDownloadVideoQuery>()),
        (CreateRealtimeCall, OpenAi) => {
            let request = if path.ends_with("/live") {
                s.input::<o::realtime::CreateLiveCallMultipartForm>()
            } else {
                s.input::<o::realtime::CreateRealtimeCallMultipartForm>()
            };
            empty()
                .request("multipart/form-data", request)
                .response("application/sdp", text())
        }
        (GenerateContent | StreamGenerateContent, OpenAiResponsesWebSocket) => {
            let server = alternatives(vec![
                s.output::<o::responses::websocket::ServerMessage>(),
                s.output::<o::responses::websocket::ErrorMessage>(),
            ]);
            Payloads {
                websocket: Some(
                    json!({"clientMessage":s.input::<o::responses::websocket::RequestMessage>(),"serverMessage":server}),
                ),
                ..empty()
            }
        }
        (ConnectRealtime, OpenAi) => Payloads {
            websocket: Some(
                json!({"clientMessage":s.input::<o::realtime::RealtimeClientEvent>(), "serverMessage":s.output::<o::realtime::RealtimeServerEvent>()}),
            ),
            parameters: vec![
                json!({"in":"query","name":"model","required":false,"schema":{"type":"string"}}),
                json!({"in":"query","name":"call_id","required":false,"schema":{"type":"string"}}),
            ],
            ..empty()
        },
        (ConnectRealtime, Gemini) => Payloads {
            websocket: Some(
                json!({"clientMessage":s.input::<g::live::LiveClientMessage>(), "serverMessage":s.output::<g::live::LiveServerMessage>()}),
            ),
            ..empty()
        },
        _ => panic!("missing payload schemas for {key:?}"),
    }
}

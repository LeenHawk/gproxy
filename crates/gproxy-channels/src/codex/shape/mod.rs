mod images;
mod responses;
pub(super) mod tools;

use bytes::Bytes;
use gproxy_channel_api::ChannelError;
use gproxy_protocol::Operation;
use gproxy_protocol::openai::common::OpenAiModelId;
use gproxy_protocol::openai::compact::CompactResponseRequestBody;
use gproxy_protocol::openai::memories::MemorySummarizeRequest;
use gproxy_protocol::openai::realtime::CreateRealtimeCallRequest;
use gproxy_protocol::openai::search::SearchRequest;

pub(super) fn request(
    operation: Operation,
    headers: &http::HeaderMap,
    body: &Bytes,
    model: &str,
) -> Result<Bytes, ChannelError> {
    match operation {
        Operation::GenerateContent
        | Operation::StreamGenerateContent
        | Operation::GuardianReview
        | Operation::GuardianClassify => responses::request(body, model),
        Operation::CreateImage => images::create(body, model),
        Operation::EditImage => images::edit(headers, body, model),
        Operation::SummarizeMemory => rewrite_memory_model(body, model),
        Operation::CompactContent => rewrite_compact_model(body, model),
        Operation::WebSearch => rewrite_search_model(body, model),
        Operation::CreateRealtimeCall
            if headers
                .get(http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.split(';').next())
                .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("application/sdp")) =>
        {
            Ok(body.clone())
        }
        Operation::CreateRealtimeCall => rewrite_realtime_model(body, model),
        _ => Ok(body.clone()),
    }
}

fn rewrite_memory_model(body: &Bytes, model: &str) -> Result<Bytes, ChannelError> {
    let mut request: MemorySummarizeRequest = serde_json::from_slice(body).map_err(|error| {
        ChannelError::Prepare(format!("memory summarize request JSON: {error}"))
    })?;
    request.model = OpenAiModelId::from(model);
    serde_json::to_vec(&request)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}

fn rewrite_realtime_model(body: &Bytes, model: &str) -> Result<Bytes, ChannelError> {
    let mut request: CreateRealtimeCallRequest = serde_json::from_slice(body)
        .map_err(|error| ChannelError::Prepare(format!("realtime call JSON: {error}")))?;
    request.model = None;
    request.session.model = Some(OpenAiModelId::from(model));
    serde_json::to_vec(&request)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}

fn rewrite_compact_model(body: &Bytes, model: &str) -> Result<Bytes, ChannelError> {
    let mut request: CompactResponseRequestBody = serde_json::from_slice(body)
        .map_err(|error| ChannelError::Prepare(format!("compact request JSON: {error}")))?;
    request.model = Some(OpenAiModelId::from(model));
    serde_json::to_vec(&request)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}

fn rewrite_search_model(body: &Bytes, model: &str) -> Result<Bytes, ChannelError> {
    let mut request: SearchRequest = serde_json::from_slice(body)
        .map_err(|error| ChannelError::Prepare(format!("search request JSON: {error}")))?;
    request
        .rest
        .insert("model".into(), serde_json::Value::String(model.into()));
    serde_json::to_vec(&request)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}

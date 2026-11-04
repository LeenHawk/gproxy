//! Route a conversion call to its family driver by operation.

use super::{Call, Converted};
use gproxy_protocol::{Operation, transform::TransformError};
use gproxy_seaorm::BatchConnectionTrait;

pub(crate) async fn dispatch<C: BatchConnectionTrait + Send + Sync + 'static>(
    call: &Call<'_, C>,
) -> Result<Converted, TransformError> {
    let result = if call.client.dialect == gproxy_protocol::Dialect::GeminiInteractions
        || call.target == gproxy_protocol::Dialect::GeminiInteractions
    {
        Box::pin(super::interactions::run(call)).await
    } else {
        match call.client.operation {
            Operation::GenerateContent => super::generate::buffered(call).await,
            Operation::StreamGenerateContent => super::generate::streamed(call).await,
            Operation::ListModels | Operation::GetModel => super::models::run(call).await,
            Operation::CountTokens => super::count_tokens::run(call).await,
            Operation::CreateEmbedding | Operation::BatchCreateEmbedding => {
                super::embeddings::run(call).await
            }
            Operation::CompactContent => super::compact::run(call).await,
            Operation::SummarizeMemory => super::memory::run(call).await,
            Operation::WebSearch => Box::pin(super::web_search::run(call)).await,
            Operation::CreateFile
            | Operation::ListFiles
            | Operation::RetrieveFile
            | Operation::RetrieveFileContent
            | Operation::DeleteFile => super::files::run(call).await,
            Operation::CreateVideo
            | Operation::RetrieveVideo
            | Operation::ListVideos
            | Operation::DeleteVideo
            | Operation::DownloadVideoContent => super::video::run(call).await,
            Operation::CreateImage | Operation::EditImage => super::images::run(call).await,
            other => Err(TransformError::unsupported(
                "route",
                format!("{other:?} has no cross-dialect conversion; it is passthrough only"),
            )),
        }
    };
    match result {
        Err(error) => {
            let Some(detail) = super::upstream_error::detail(&error) else {
                return Err(error);
            };
            let status = super::upstream_error::status(&detail);
            let Some(body) = super::upstream_error::body(&detail, status, call.client.dialect)
            else {
                return Err(error);
            };
            Ok(Converted::Rejected(gproxy_protocol::WireResponse {
                status: http::StatusCode::from_u16(status).expect("error HTTP status"),
                headers: http::HeaderMap::from_iter([(
                    http::header::CONTENT_TYPE,
                    http::HeaderValue::from_static("application/json"),
                )]),
                body: gproxy_protocol::HttpBody::Bytes(body.to_string().into()),
            }))
        }
        Ok(Converted::Rejected(response)) if call.client.dialect != call.target => {
            Ok(prompt_too_long(call, response))
        }
        result => result,
    }
}

/// An over-long prompt refused by an upstream of the other side, in the
/// words the client compacts on. A streamed Responses turn ends in
/// `response.failed`, where Codex reads it; every other client gets the
/// error body of its own dialect. Other refusals stay as the upstream sent.
fn prompt_too_long<C: BatchConnectionTrait + Send + Sync + 'static>(
    call: &Call<'_, C>,
    response: gproxy_protocol::WireResponse<gproxy_protocol::HttpBody>,
) -> Converted {
    let gproxy_protocol::HttpBody::Bytes(bytes) = &response.body else {
        return Converted::Rejected(response);
    };
    let generation = matches!(
        call.client.operation,
        Operation::GenerateContent | Operation::StreamGenerateContent
    );
    let Some(detail) = generation
        .then(|| super::upstream_error::prompt_too_long(bytes, call.target, call.client.dialect))
        .flatten()
    else {
        return Converted::Rejected(response);
    };
    if call.client.dialect == gproxy_protocol::Dialect::OpenAi
        && call.client.operation == Operation::StreamGenerateContent
    {
        let event = super::upstream_error::failed_event(&detail);
        let frame = format!("event: response.failed\ndata: {event}\n\n");
        return Converted::Stream(gproxy_protocol::WireResponse {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::from_iter([(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_static("text/event-stream"),
            )]),
            body: Box::pin(futures_util::stream::iter([Ok(frame.into())])),
        });
    }
    let Some(body) =
        super::upstream_error::body(&detail, response.status.as_u16(), call.client.dialect)
    else {
        return Converted::Rejected(response);
    };
    Converted::Rejected(gproxy_protocol::WireResponse {
        status: response.status,
        headers: http::HeaderMap::from_iter([(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/json"),
        )]),
        body: gproxy_protocol::HttpBody::Bytes(body.to_string().into()),
    })
}

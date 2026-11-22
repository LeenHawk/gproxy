//! Route a conversion call to its family driver by operation.

use crate::{Call, Converted, Host};
use gproxy_protocol::{Operation, transform::TransformError};

pub async fn dispatch<H: Host>(call: &Call<'_, H>) -> Result<Converted, TransformError> {
    let result = if call.client.dialect == gproxy_protocol::Dialect::GeminiInteractions
        || call.target == gproxy_protocol::Dialect::GeminiInteractions
    {
        Box::pin(crate::interactions::run(call)).await
    } else {
        match call.client.operation {
            Operation::GenerateContent => crate::generate::buffered(call).await,
            Operation::StreamGenerateContent => crate::generate::streamed(call).await,
            Operation::ListModels | Operation::GetModel => crate::models::run(call).await,
            Operation::CountTokens => crate::count_tokens::run(call).await,
            Operation::CreateEmbedding | Operation::BatchCreateEmbedding => {
                crate::embeddings::run(call).await
            }
            Operation::CompactContent => crate::compact::run(call).await,
            Operation::SummarizeMemory => crate::memory::run(call).await,
            Operation::WebSearch => Box::pin(crate::web_search::run(call)).await,
            Operation::CreateFile
            | Operation::ListFiles
            | Operation::RetrieveFile
            | Operation::RetrieveFileContent
            | Operation::DeleteFile => crate::files::run(call).await,
            Operation::CreateVideo
            | Operation::RetrieveVideo
            | Operation::ListVideos
            | Operation::DeleteVideo
            | Operation::DownloadVideoContent => crate::video::run(call).await,
            Operation::CreateImage | Operation::EditImage => crate::images::run(call).await,
            other => Err(TransformError::unsupported(
                "route",
                format!("{other:?} has no cross-dialect conversion; it is passthrough only"),
            )),
        }
    };
    match result {
        Err(error) => {
            let Some(detail) = crate::upstream_error::detail(&error) else {
                return Err(error);
            };
            let status = crate::upstream_error::status(&detail);
            let Some(body) = crate::upstream_error::body(&detail, status, call.client.dialect)
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
fn prompt_too_long<H: Host>(
    call: &Call<'_, H>,
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
        .then(|| crate::upstream_error::prompt_too_long(bytes, call.target, call.client.dialect))
        .flatten()
    else {
        return Converted::Rejected(response);
    };
    if call.client.dialect == gproxy_protocol::Dialect::OpenAi
        && call.client.operation == Operation::StreamGenerateContent
    {
        let event = crate::upstream_error::failed_event(&detail);
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
        crate::upstream_error::body(&detail, response.status.as_u16(), call.client.dialect)
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

//! Route a conversion call to its family driver by operation.

use super::{Call, Converted};
use gproxy_protocol::{Operation, transform::TransformError};
use gproxy_seaorm::BatchConnectionTrait;

pub(crate) async fn dispatch<C: BatchConnectionTrait + Send + Sync + 'static>(
    call: &Call<'_, C>,
) -> Result<Converted, TransformError> {
    let result = match call.client.operation {
        Operation::GenerateContent => super::generate::buffered(call).await,
        Operation::StreamGenerateContent => super::generate::streamed(call).await,
        Operation::ListModels | Operation::GetModel => super::models::run(call).await,
        Operation::CountTokens => super::count_tokens::run(call).await,
        Operation::CreateEmbedding | Operation::BatchCreateEmbedding => {
            super::embeddings::run(call).await
        }
        Operation::GuardianReview | Operation::GuardianClassify => super::guardian::run(call).await,
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
        result => result,
    }
}

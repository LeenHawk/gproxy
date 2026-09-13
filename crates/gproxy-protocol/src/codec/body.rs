use bytes::Bytes;
use futures_util::StreamExt;

use super::{CodecError, CodecErrorKind, CodecErrorStage, CodecFuture, CodecLimits};
use crate::connection::HttpBody;

/// Consumes one HTTP body under an aggregate byte limit.
///
/// A stream is consumed incrementally and its transport source is retained on
/// failure. This helper is intentionally the only operation here that buffers
/// an entire body; callers must supply a finite `max_body_bytes`.
pub fn read_http_body(
    body: HttpBody,
    limits: CodecLimits,
) -> CodecFuture<'static, Result<Bytes, CodecError>> {
    Box::pin(async move {
        match body {
            HttpBody::Bytes(bytes) => {
                if bytes.len() as u64 > limits.max_body_bytes {
                    return Err(CodecError::new(
                        CodecErrorKind::Limit,
                        CodecErrorStage::Body,
                        "HTTP body exceeds limit",
                    ));
                }
                Ok(bytes)
            }
            HttpBody::Stream(mut stream) => {
                let mut output = Vec::new();
                while let Some(chunk) = stream.next().await {
                    let chunk = chunk.map_err(|source| match source.downcast::<CodecError>() {
                        Ok(error) => *error,
                        Err(source) => CodecError::with_source(
                            CodecErrorKind::Transport,
                            CodecErrorStage::Body,
                            "HTTP body stream failed",
                            source,
                        ),
                    })?;
                    let next = output.len().saturating_add(chunk.len());
                    if next as u64 > limits.max_body_bytes {
                        return Err(CodecError::new(
                            CodecErrorKind::Limit,
                            CodecErrorStage::Body,
                            "HTTP body exceeds limit",
                        ));
                    }
                    output.extend_from_slice(&chunk);
                }
                Ok(Bytes::from(output))
            }
        }
    })
}

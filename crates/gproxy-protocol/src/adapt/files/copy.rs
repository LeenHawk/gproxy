use crate::{
    WireRequest,
    adapt::{
        JsonInvocation,
        files::{UploadError, upload_multipart_json},
    },
    capability::{ResourceAccess, ResourceReference, Upstream},
    codec::{self, CodecLimits},
    connection::{HttpBody, MultipartPart},
    transform::TransformError,
    wire::DeclaredFields,
};
/// Downloads a source resource and sends an actual selected Files upload.
/// This does not use ResourceAccess::publish or treat publication as file upload.
/// Source is fully bounded/preflighted before the target side effect begins.
#[allow(clippy::too_many_arguments)] // Separate source/destination capabilities and scoped targets.
pub async fn copy_to_multipart<
    A: ResourceAccess,
    U: Upstream,
    O: serde::de::DeserializeOwned + DeclaredFields,
>(
    access: &A,
    source_scope: &A::Scope,
    source: &ResourceReference,
    upstream: &U,
    target: &U::Target,
    template: WireRequest<()>,
    boundary: String,
    mut fields: Vec<MultipartPart>,
    filename: Option<String>,
    limits: CodecLimits,
) -> Result<JsonInvocation<O>, UploadError> {
    super::path(&template.path)?;
    let read = access
        .read(source_scope, source)
        .await
        .map_err(TransformError::from)?;
    let filename = filename
        .or(read.metadata.filename)
        .ok_or_else(|| TransformError::missing_metadata("file.filename"))?;
    if filename.contains(['\r', '\n', '\0']) {
        return Err(TransformError::shape("filename", "invalid header characters").into());
    }
    let mime = read
        .metadata
        .mime
        .ok_or_else(|| TransformError::missing_metadata("file.mime"))?;
    let mut read_limits = limits;
    read_limits.max_body_bytes = read_limits
        .max_body_bytes
        .min(access.limits().read_bytes)
        .min(limits.max_part_bytes);
    read_limits.max_buffer_bytes = read_limits.max_buffer_bytes.min(read_limits.max_body_bytes);
    let bytes = codec::read_http_body(read.body, read_limits)
        .await
        .map_err(|e| {
            TransformError::new(
                if e.kind() == codec::CodecErrorKind::Limit {
                    crate::transform::TransformErrorKind::Limit
                } else {
                    crate::transform::TransformErrorKind::Host
                },
                "file.copy.read",
                e.to_string(),
            )
        })?;
    if read
        .metadata
        .length
        .is_some_and(|length| length != bytes.len() as u64)
    {
        return Err(TransformError::invalid_result(
            "file.copy.length",
            "source body differs from declared resource length",
        )
        .into());
    }
    let escaped = filename.replace('\\', "\\\\").replace('"', "\\\"");
    let mut headers = http::HeaderMap::new();
    headers.insert(
        http::header::CONTENT_DISPOSITION,
        http::HeaderValue::from_str(&format!("form-data; name=\"file\"; filename=\"{escaped}\""))
            .map_err(|e| TransformError::shape("filename", e.to_string()))?,
    );
    headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_str(&mime)
            .map_err(|e| TransformError::shape("mime", e.to_string()))?,
    );
    fields.push(MultipartPart {
        headers,
        body: HttpBody::Bytes(bytes),
    });
    upload_multipart_json(upstream, target, template, boundary, fields, limits).await
}

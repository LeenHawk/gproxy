//! Scoped, bounded media reads for targets which require inline attachments.
//! All policy, execution dependencies, target controls and known media shapes
//! are validated before a read. Native URL-capable targets retain their URLs.

use crate::{
    capability::{ResourceAccess, ResourceReference},
    codec::{self, CodecLimits},
    transform::{
        Converted, TransformError, TransformErrorKind,
        guardian::{self, GuardianPreparedRequest, GuardianRequestContext, GuardianTarget},
    },
    wire::openai::guardian::GuardianRequestBody,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy)]
pub struct GuardianResourceLimits {
    pub codec: CodecLimits,
    pub max_media: usize,
    pub max_resource_bytes: u64,
    pub max_total_resource_bytes: u64,
}

fn limit(field: &str) -> TransformError {
    TransformError::new(
        TransformErrorKind::Limit,
        field,
        "Guardian media exceeds configured cap",
    )
}

/// Prepare one request using the caller's authorized resource scope. No model
/// invocation or publication occurs here; cancellation cannot trigger a retry.
/// Source evidence retains its original references for subsequent SSE binding.
pub async fn prepare_with_resources<A: ResourceAccess>(
    access: &A,
    scope: &A::Scope,
    input: GuardianRequestBody,
    target: GuardianTarget,
    context: GuardianRequestContext,
    limits: GuardianResourceLimits,
) -> Result<Converted<GuardianPreparedRequest>, TransformError> {
    let (mut prepared, mut media) =
        guardian::prepare_unattached(input, target, context, limits.codec)?;
    if media.len() > limits.max_media {
        return Err(limit("guardian.media.count"));
    }
    let mut total = 0u64;
    for part in &media {
        if let Some((_, data)) = part
            .url()
            .strip_prefix("data:")
            .and_then(|v| v.split_once(";base64,"))
        {
            let bytes = STANDARD
                .decode(data)
                .map_err(|e| TransformError::shape("guardian.media", e.to_string()))?;
            let len = bytes.len() as u64;
            if len > limits.max_resource_bytes {
                return Err(limit("guardian.media.bytes"));
            }
            total = total
                .checked_add(len)
                .ok_or_else(|| limit("guardian.media.total"))?;
        }
    }
    if total > limits.max_total_resource_bytes {
        return Err(limit("guardian.media.total"));
    }
    // Per-reference deduplication preserves distinct attachment ordinals while
    // charging all embedded copies against the aggregate delivery budget.
    let mut resolved: HashMap<String, (String, u64)> = HashMap::new();
    for part in &mut media {
        if !part.needs_resolution(target) {
            continue;
        }
        let original = part.url().to_owned();
        if let Some((uri, len)) = resolved.get(&original) {
            total = total
                .checked_add(*len)
                .ok_or_else(|| limit("guardian.media.total"))?;
            if total > limits.max_total_resource_bytes {
                return Err(limit("guardian.media.total"));
            }
            part.set_url(uri.clone());
            continue;
        }
        let cap = limits
            .max_resource_bytes
            .min(limits.max_total_resource_bytes - total)
            .min(access.limits().read_bytes)
            .min(limits.codec.max_body_bytes);
        if cap == 0 {
            return Err(limit("guardian.media.read"));
        }
        let read = access
            .read(scope, &ResourceReference::Url(original.clone()))
            .await
            .map_err(TransformError::from)?;
        if read.metadata.length.is_some_and(|n| n > cap) {
            return Err(limit("guardian.media.length"));
        }
        let mime = read
            .metadata
            .mime
            .ok_or_else(|| TransformError::missing_metadata("guardian.media.mime"))?;
        let bytes = codec::read_http_body(
            read.body,
            CodecLimits {
                max_body_bytes: cap,
                ..limits.codec
            },
        )
        .await
        .map_err(|e| {
            TransformError::new(
                if e.kind() == codec::CodecErrorKind::Limit {
                    TransformErrorKind::Limit
                } else {
                    TransformErrorKind::Host
                },
                "guardian.media.read",
                e.to_string(),
            )
        })?;
        let len = bytes.len() as u64;
        if read.metadata.length.is_some_and(|n| n != len) {
            return Err(TransformError::invalid_result(
                "guardian.media.length",
                "host metadata differs from actual bytes",
            ));
        }

        let uri = format!("data:{mime};base64,{}", STANDARD.encode(&bytes));
        total += len;
        resolved.insert(original, (uri.clone(), len));
        part.set_url(uri);
    }
    guardian::finish_attachments(&mut prepared.value, &media, limits.codec)?;
    if !resolved.is_empty() {
        prepared.report.changed(
            "guardian.media",
            "scoped ResourceAccess reads materialized native inline attachments",
        );
    }
    Ok(prepared)
}

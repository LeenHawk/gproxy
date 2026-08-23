use gproxy_protocol::{
    Affinity, Operation, OperationKey, SettleMode, StreamDetect, match_ingress, streaming_sibling,
};

use crate::boundary::RequestCtx;
use crate::error::CoreError;

pub(crate) struct Classified {
    pub key: OperationKey,
    pub stream: bool,
    pub settle: SettleMode,
    pub model: Option<String>,
    resource: Option<(&'static str, String)>,
}

impl Classified {
    pub(crate) fn dedupe_key(&self, provider_id: i64) -> Option<String> {
        self.resource
            .as_ref()
            .map(|(kind, id)| format!("gproxy:settle:{provider_id}:{kind}:{id}"))
    }
}

pub(crate) fn classify(ctx: &RequestCtx) -> Result<Classified, CoreError> {
    let matched = match_ingress(&ctx.method, &ctx.path).ok_or(CoreError::Unsupported)?;
    if matched.upgrade {
        return Err(CoreError::Unsupported);
    }
    let body = serde_json::from_slice::<serde_json::Value>(&ctx.body).ok();
    let stream = match matched.stream {
        StreamDetect::Never => false,
        StreamDetect::Always => true,
        StreamDetect::BodyFlag(field) => body
            .as_ref()
            .and_then(|body| body.get(field)?.as_bool())
            .unwrap_or(false),
    };
    let operation = if stream {
        streaming_sibling(matched.operation).unwrap_or(matched.operation)
    } else {
        matched.operation
    };
    let spec = operation.spec();
    let model = matched
        .params
        .iter()
        .find(|(name, _)| *name == "model")
        .or_else(|| {
            (operation == Operation::GetModel)
                .then(|| matched.params.first())
                .flatten()
        })
        .map(|(_, value)| value.clone())
        .or_else(|| {
            body.as_ref()
                .and_then(|body| body.get("model")?.as_str().map(str::to_owned))
        });
    let resource = match (spec.settle, spec.affinity, matched.params.first()) {
        (SettleMode::OnCompletedStatus, Affinity::Resource(kind), Some((_, id))) => {
            Some((kind, id.clone()))
        }
        _ => None,
    };
    Ok(Classified {
        key: OperationKey {
            operation,
            kind: matched.kind,
        },
        stream,
        settle: spec.settle,
        model,
        resource,
    })
}

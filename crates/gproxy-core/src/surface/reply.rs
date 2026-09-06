use bytes::Bytes;
use gproxy_channel_api::{SurfaceBody, SurfaceReply, SurfaceRequest, TransportError};

use crate::boundary::{ExecOutcome, RequestCtx, ResponseBody, RoutingMode};
use crate::control::Target;
use crate::error::CoreError;

pub(crate) fn request_ctx(
    target: &Target,
    request: &SurfaceRequest,
    request_id: String,
) -> RequestCtx {
    RequestCtx {
        request_id,
        client_ip: None,
        method: request.method.clone(),
        path: request.upstream_path.clone(),
        query: request.query.clone(),
        headers: request.headers.clone(),
        body: request.body.clone(),
        upgrade: false,
        force_model_refresh: false,
        mode: RoutingMode::Scoped {
            provider: target.provider.name.clone(),
        },
    }
}

pub(crate) fn from_outcome(outcome: ExecOutcome) -> Result<SurfaceReply, TransportError> {
    let body = match outcome.body {
        ResponseBody::Full(body) => SurfaceBody::Full(body),
        ResponseBody::Stream(body) => SurfaceBody::Stream(body),
        ResponseBody::WebSocket(_) => {
            return Err(TransportError::Interrupted(
                "surface invoke returned a websocket".into(),
            ));
        }
    };
    Ok(SurfaceReply {
        status: outcome.status,
        headers: outcome.headers,
        body,
    })
}

pub(crate) fn error(error: CoreError) -> SurfaceReply {
    SurfaceReply {
        status: error.status(),
        headers: http::HeaderMap::from_iter([(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/json"),
        )]),
        body: SurfaceBody::Full(Bytes::from(error.body_json().to_string())),
    }
}

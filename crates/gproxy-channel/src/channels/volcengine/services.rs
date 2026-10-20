//! get-personal-plan-api: read-only plan status, not a quota reading.
use super::{Volcengine, control, json_response};
use crate::channel::{
    ChannelError, ChannelServices, OperationFuture, ServiceClass, ServiceContext, ServiceRoute,
    ServiceTransport,
};
use gproxy_protocol::{HttpBody, WireResponse};
use http::{Method, StatusCode};
use serde_json::json;

const ROUTES: &[ServiceRoute] = &[ServiceRoute {
    method: Method::GET,
    path_template: "/api/volcengine/plan",
    transport: ServiceTransport::Http,
    idempotent: true,
    class: ServiceClass::Restricted,
}];

impl ChannelServices for Volcengine {
    fn routes(&self) -> &[ServiceRoute] {
        ROUTES
    }

    fn call<'a>(&'a self, ctx: ServiceContext<'a>) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(async move {
            if self.route(&ctx.request.method, &ctx.request.path).is_none() {
                return Err(ChannelError::UnsupportedService);
            }
            if ctx.view.is_synthesized() {
                return Ok(json_response(
                    StatusCode::FORBIDDEN,
                    json!({"error": {"type": "permission_error", "message": "Ark plan information requires an administrative credential view"}}),
                ));
            }
            let result = control::call(
                ctx.account,
                "GetPersonalPlan",
                json!({
                    "Plan": if self.agent { "AgentPlan" } else { "CodingPlan" },
                }),
                None,
            )
            .await?;
            Ok(json_response(StatusCode::OK, result))
        })
    }
}

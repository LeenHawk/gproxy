use super::*;
use crate::documentation::Api;
use gproxy_app::dto as a;
use serde_json::json;

pub(crate) fn document(api: &mut Api) {
    for path in [
        "/v1/.well-known/oauth-authorization-server",
        "/.well-known/oauth-authorization-server/v1",
        "/.well-known/oauth-authorization-server/{mount}/v1",
        "/v1/oauth/metadata",
    ] {
        api.get::<a::AuthorizationServerMetadata>(path, "")["security"] = json!([]);
    }
    let start = api.query::<a::AuthorizeQuery, a::AuthorizeDetails>("/v1/oauth/authorize", "");
    start["responses"]["302"] = json!({"description":"Browser consent handoff","headers":{"Location":{"schema":{"type":"string"}}}});
    start["security"].as_array_mut().unwrap().push(json!({}));
    start["description"] = json!(
        "Accept: text/html redirects to consent without authentication. JSON details require a console session or management API key."
    );
    let request = api.schemas.input::<DecisionBody>();
    let query = api.schemas.query::<a::AuthorizeQuery>();
    let outcome = api.schemas.output::<a::AuthorizeOutcome>();
    api.route("post","/v1/oauth/authorize","",Some(request),query,Some(json!({"type":"object","required":["location","outcome"],"properties":{"location":{"type":"string"},"outcome":outcome}})));
    api.json::<a::TokenRequest, a::TokenResponse>("post", "/v1/oauth/token", "");
    api.json::<a::DeviceCodeRequest, a::DeviceCodeResponse>("post", "/v1/oauth/device/code", "");
    let revoke = api.schemas.input::<a::RevokeRequest>();
    api.route("post","/v1/oauth/revoke","",Some(revoke),vec![],Some(json!({"type":"object","required":["revoked"],"properties":{"revoked":{"type":"boolean"}}})));
    for path in [
        "/v1/oauth/token",
        "/v1/oauth/device/code",
        "/v1/oauth/revoke",
    ] {
        let operation = &mut api.document["paths"][path]["post"];
        let body = operation["requestBody"]["content"]["application/json"].clone();
        operation["requestBody"]["content"]["application/x-www-form-urlencoded"] = body;
        operation["security"] = json!([]);
    }
    let error = api.schemas.output::<a::OAuthErrorBody>();
    for (path, methods) in api.document["paths"].as_object_mut().unwrap() {
        if path.contains("/oauth/") || path.contains("/.well-known/") {
            if path.starts_with("/portal/") {
                continue;
            }
            for operation in methods.as_object_mut().unwrap().values_mut() {
                operation["responses"]["default"] = json!({"description":"OAuth error","content":{"application/json":{"schema":error}}});
            }
        }
    }
}

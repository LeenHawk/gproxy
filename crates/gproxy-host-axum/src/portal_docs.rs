use super::*;
use crate::documentation::Api;
use gproxy_app::dto as a;
use serde_json::json;

pub(crate) fn document(api: &mut Api) {
    let request = api.schemas.input::<LoginBody>();
    let login=api.route("post","/portal/api/login","",Some(request),vec![],Some(json!({
        "type":"object","required":["sessionId","token","expiresAtMs"],
        "properties":{"sessionId":{"type":"string"},"token":{"type":"string"},"expiresAtMs":{"type":"integer","format":"int64"}}
    })));
    login["security"] = json!([]);
    login["responses"]["200"]["headers"] =
        json!({"Set-Cookie":{"description":"Console session cookie","schema":{"type":"string"}}});
    api.route("post","/portal/api/logout","",None,vec![],Some(json!({"type":"object","required":["endedSession"],"properties":{"endedSession":{"type":"boolean"}}})))["security"]=json!([]);
    api.get::<a::PortalContextDto>("/portal/api/context", "");
    api.get::<Vec<a::PortalModelDto>>("/portal/api/models", "");
    api.query::<a::PortalUsageQuery, a::PortalUsageDto>("/portal/api/usage", "");
    api.get::<Vec<a::PortalQuotaWindowDto>>("/portal/api/quota", "");
    api.get::<Vec<a::PortalRequestDto>>("/portal/api/requests", "");
    api.get::<Vec<a::UserSessionDto>>("/portal/api/sessions", "");
    api.get::<Vec<a::PortalKeyDto>>("/portal/api/keys", "");
    api.json::<a::PortalKeyCreate, a::PortalKeyCreated>("post", "/portal/api/keys", "");
    api.empty("delete", "/portal/api/keys/{id}", "");
    api.action::<a::PortalKeyCreated>("/portal/api/keys/{id}/rotate", "");
    api.get::<a::PortalKeySecretDto>("/portal/api/keys/{id}/secret", "");
    api.get::<Vec<a::PortalOAuthSessionDto>>("/portal/api/oauth-sessions", "");
    api.empty("delete", "/portal/api/oauth-sessions/{id}", "");
    api.write_empty::<a::PortalPasswordChange>("post", "/portal/api/password", "");
    api.query::<DeviceQuery, a::DeviceDetails>("/portal/api/oauth/device", "");
    api.json::<DeviceDecisionBody, a::DeviceDecided>("post", "/portal/api/oauth/device", "");
}

use super::*;
use crate::documentation::Api;

pub(crate) fn document(api: &mut Api) {
    api.get::<AdminContextDto>("/admin/api/context", "");
    api.get::<SessionStatus>("/admin/api/session", "");
    api.route("delete","/admin/api/session","",None,vec![],Some(serde_json::json!({"type":"object","required":["endedSession"],"properties":{"endedSession":{"type":"boolean"}}})));
}

use super::*;
use crate::documentation::Api;
use serde_json::json;

pub(crate) fn document(api: &mut Api) {
    api.get::<UpdateSchedule>("/admin/api/update", SECTION);
    api.get::<Option<UpdateProgress>>("/admin/api/update/progress", SECTION);
    let query = api.schemas.query::<ChannelQuery>();
    let response = api.schemas.output::<UpdateReport>();
    api.route(
        "post",
        "/admin/api/update/check",
        SECTION,
        None,
        query,
        Some(response),
    );
    let query = api.schemas.query::<ChannelQuery>();
    let response = api.schemas.output::<AppliedUpdate>();
    api.route(
        "post",
        "/admin/api/update/apply",
        SECTION,
        None,
        query,
        Some(response),
    );
    api.action::<AppliedUpdate>("/admin/api/update/rollback", SECTION);
    api.get::<Vec<Announcement>>("/admin/api/update/announcements", SECTION);
    for (path, methods) in api.document["paths"].as_object_mut().unwrap() {
        if path.starts_with("/admin/api/update") {
            for operation in methods.as_object_mut().unwrap().values_mut() {
                operation["x-runtime-condition"] = json!("Host with an update service configured");
            }
        }
    }
}

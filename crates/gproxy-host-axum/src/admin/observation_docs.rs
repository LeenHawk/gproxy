use crate::documentation::Api;
use gproxy_app::dto as a;
use gproxy_sdk::dto as d;

pub(crate) fn document(api: &mut Api) {
    api.query::<a::PortalUsageQuery, a::PortalUsageDto>("/admin/api/usage", "usage");
    api.query::<d::UsageRecordQuery, d::UsageRecordPage>("/admin/api/usage/records", "usage");
    for (path, section) in [
        ("/usage/records", "usage"),
        ("/logs/downstream", "logs"),
        ("/logs/upstream", "logs"),
    ] {
        let response = api.schemas.output::<d::HistoryDeleted>();
        api.route(
            "delete",
            &format!("/admin/api{path}"),
            section,
            None,
            vec![],
            Some(response),
        );
        api.json::<d::HistoryDelete, d::HistoryDeleted>(
            "post",
            &format!("/admin/api{path}/delete"),
            section,
        );
    }
    api.query::<d::LogQuery, d::LogPageDto>("/admin/api/logs/downstream", "logs");
    api.query::<d::LogQuery, d::LogPageDto>("/admin/api/logs/upstream", "logs");
    api.get::<d::LogDetailDto>("/admin/api/logs/downstream/{id}", "logs");
    api.get::<d::CaptureDetailDto>("/admin/api/logs/captures/{id}", "logs");
}

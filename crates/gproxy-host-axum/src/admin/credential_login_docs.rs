use super::*;
use crate::documentation::Api;
use gproxy_app::dto as a;
use gproxy_sdk::dto as d;

pub(crate) fn document(api: &mut Api) {
    api.get::<Vec<a::CredentialProviderDto>>("/admin/api/credentials/providers", "credentials");
    api.get::<Vec<a::CredentialOwnerOptionDto>>("/admin/api/credentials/owners", "credentials");
    api.action::<Vec<d::DiscoveredModelDto>>(
        "/admin/api/credentials/{id}/models/discover",
        "credentials",
    );
    api.json::<Model, d::ModelTestResultDto>(
        "post",
        "/admin/api/credentials/{id}/models/test",
        "credentials",
    );
    api.json::<d::AuthCodeStart, d::AuthCodeStarted>(
        "post",
        "/admin/api/credential-login/authcode/start",
        "credentials",
    );
    api.json::<d::AuthCodeComplete, d::CredentialCreated>(
        "post",
        "/admin/api/credential-login/authcode/complete",
        "credentials",
    );
    api.json::<d::DeviceStart, d::DeviceStarted>(
        "post",
        "/admin/api/credential-login/device/start",
        "credentials",
    );
    api.json::<Poll, d::DevicePollOutcome>(
        "post",
        "/admin/api/credential-login/device/poll",
        "credentials",
    );
}

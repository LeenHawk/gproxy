use super::*;
use crate::documentation::Api;
use gproxy_app::dto as a;
use serde_json::json;

pub(crate) fn document(api: &mut Api) {
    macro_rules! family {
        ($path:literal, $write:ty, $patch:ty, $read:ty, $created:ty, $section:literal) => {{
            api.query::<a::ListQuery, a::Page<$read>>(concat!("/admin/api", $path), $section);
            api.json::<$write, $created>("post", concat!("/admin/api", $path), $section);
            api.json::<Vec<a::BatchItem<$write, $patch>>, Vec<Option<$read>>>(
                "post",
                concat!("/admin/api", $path, "/batch"),
                $section,
            );
            api.get::<$read>(concat!("/admin/api", $path, "/{id}"), $section);
            api.json::<$patch, $read>("patch", concat!("/admin/api", $path, "/{id}"), $section);
        }};
    }
    family!(
        "/users",
        a::UserWrite,
        a::UserPatch,
        a::UserDto,
        a::UserDto,
        "users"
    );
    family!(
        "/api-keys",
        a::ApiKeyWrite,
        a::ApiKeyPatch,
        a::ApiKeyDto,
        a::ApiKeyCreated,
        "api-keys"
    );
    family!(
        "/organizations",
        a::OrganizationWrite,
        a::OrganizationPatch,
        a::OrganizationDto,
        a::OrganizationDto,
        "organizations"
    );
    family!(
        "/teams",
        a::TeamWrite,
        a::TeamPatch,
        a::TeamDto,
        a::TeamDto,
        "teams"
    );
    family!(
        "/permissions",
        a::PermissionWrite,
        a::PermissionPatch,
        a::PermissionDto,
        a::PermissionDto,
        "permissions"
    );
    family!(
        "/rate-limits",
        a::RateLimitWrite,
        a::RateLimitPatch,
        a::RateLimitDto,
        a::RateLimitDto,
        "rate-limits"
    );
    family!(
        "/oauth-clients",
        a::OAuthClientWrite,
        a::OAuthClientPatch,
        a::OAuthClientDto,
        a::OAuthClientDto,
        "oauth-clients"
    );
    for section in [
        "users",
        "api-keys",
        "organizations",
        "teams",
        "permissions",
        "rate-limits",
    ] {
        api.empty("delete", &format!("/admin/api/{section}/{{id}}"), section);
    }
    api.json::<PasswordBody, a::UserDto>("post", "/admin/api/users/{id}/password", "users");
    let user = api.schemas.output::<a::UserDto>();
    api.route(
        "delete",
        "/admin/api/users/{id}/password",
        "users",
        None,
        vec![],
        Some(user),
    );
    api.json::<AllowlistBody, a::UserDto>("put", "/admin/api/users/{id}/allowlist", "users");
    api.action::<a::ApiKeyCreated>("/admin/api/api-keys/{id}/rotate", "api-keys");
    api.get::<a::ApiKeySecretDto>("/admin/api/api-keys/{id}/secret", "api-keys");
    api.action::<a::OAuthClientDto>("/admin/api/oauth-clients/{id}/retire", "oauth-clients");

    macro_rules! members {
        ($path:literal,$read:ty,$section:literal) => {{
            api.query::<a::ListQuery, a::Page<$read>>(
                concat!("/admin/api/", $path, "/{id}/members"),
                $section,
            );
            api.json::<a::MemberWrite, $read>(
                "post",
                concat!("/admin/api/", $path, "/{id}/members"),
                $section,
            );
            api.get::<$read>(
                concat!("/admin/api/", $path, "/{id}/members/{user_id}"),
                $section,
            );
            api.json::<a::MemberPatch, $read>(
                "put",
                concat!("/admin/api/", $path, "/{id}/members/{user_id}"),
                $section,
            );
            api.empty(
                "delete",
                concat!("/admin/api/", $path, "/{id}/members/{user_id}"),
                $section,
            );
        }};
    }
    members!("organizations", a::OrganizationMemberDto, "members");
    members!("teams", a::TeamMemberDto, "team-members");
    api.query::<a::ListQuery, a::Page<a::UserSessionDto>>("/admin/api/sessions", "sessions");
    api.empty("delete", "/admin/api/sessions/{id}", "sessions");
    api.get::<a::Page<a::UserSessionDto>>("/admin/api/users/{id}/sessions", "sessions");
    api.route("delete","/admin/api/users/{id}/sessions","sessions",None,vec![],Some(json!({"type":"object","required":["revoked"],"properties":{"revoked":{"type":"integer","minimum":0}}})));
    api.query::<a::AuditQuery, a::Page<a::AuditEventDto>>("/admin/api/audit", "audit");
}

use gproxy_channel_api::SurfaceAffinity;

pub(super) const TOKEN_NAMESPACE: &str = "codex_remote";
pub(super) const REMOTE_TTL_SECS: u64 = 7 * 24 * 60 * 60;
pub(super) const MCP_TTL_SECS: u64 = 24 * 60 * 60;
pub(super) const PLUGIN_TTL_SECS: u64 = 30 * 24 * 60 * 60;

pub(super) const REMOTE_CREATE: SurfaceAffinity = SurfaceAffinity::ResponseBodyToken {
    field: "remote_control_token",
    namespace: TOKEN_NAMESPACE,
    request_body_field: None,
    also_body_field: Some("server_id"),
    also_path_field: Some("environment_id"),
    ttl_secs: REMOTE_TTL_SECS,
};

pub(super) const REMOTE_REFRESH: SurfaceAffinity = SurfaceAffinity::ResponseBodyToken {
    field: "remote_control_token",
    namespace: TOKEN_NAMESPACE,
    request_body_field: Some("server_id"),
    also_body_field: Some("server_id"),
    also_path_field: Some("environment_id"),
    ttl_secs: REMOTE_TTL_SECS,
};

pub(super) const REMOTE_HTTP: SurfaceAffinity = SurfaceAffinity::HeaderOrBodyField {
    header: "x-codex-server-id",
    body_field: "server_id",
    ttl_secs: REMOTE_TTL_SECS,
};

pub(super) const REMOTE_SOCKET: SurfaceAffinity = SurfaceAffinity::BearerToken {
    namespace: TOKEN_NAMESPACE,
};

pub(super) const REMOTE_ENVIRONMENT: SurfaceAffinity = SurfaceAffinity::PathParam {
    name: "environment_id",
    ttl_secs: REMOTE_TTL_SECS,
};

pub(super) const MCP: SurfaceAffinity = SurfaceAffinity::Header {
    name: "mcp-session-id",
    ttl_secs: MCP_TTL_SECS,
};

pub(super) const PLUGIN: SurfaceAffinity = SurfaceAffinity::PathParam {
    name: "plugin_id",
    ttl_secs: PLUGIN_TTL_SECS,
};

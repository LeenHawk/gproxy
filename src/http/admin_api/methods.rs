//! Method discovery for known admin and portal routes.

pub(super) fn allowed_methods(segments: &[&str]) -> Option<&'static str> {
    match segments {
        ["admin", "login" | "logout"] => Some("POST"),
        ["admin", "me" | "channels" | "notifications"] => Some("GET,HEAD"),
        ["admin", "autostart"] => Some("GET,HEAD,PUT"),
        [
            "admin",
            "orgs" | "providers" | "routes" | "aliases" | "price-rules" | "rule-sets"
            | "instance-settings" | "users",
        ] => Some("GET,HEAD,POST"),
        [
            "admin",
            "usage-summary"
            | "usage-rollups"
            | "credential-statuses"
            | "credential-model-statuses"
            | "tls-presets",
        ] => Some("GET,HEAD"),
        ["admin", "usage" | "logs" | "audit"] => Some("GET,HEAD,DELETE"),
        [
            "admin",
            "credential-usage-comparison" | "credential-quota-cycles",
        ] => Some("GET,HEAD"),
        ["admin", "credential-quota-cycles", _] => Some("GET,HEAD"),
        ["admin", "batch", _] => Some("POST"),
        [
            "admin",
            "orgs" | "providers" | "routes" | "aliases" | "price-rules" | "rule-sets" | "users",
            _,
        ] => Some("GET,HEAD,DELETE"),
        [
            "admin",
            "credentials" | "user-keys" | "teams" | "provider-models" | "route-members" | "rules"
            | "routing-rules" | "provider-rule-sets" | "route-permissions" | "rate-limits"
            | "quotas",
            _,
        ] => Some("DELETE"),
        ["admin", "route-permissions" | "rate-limits" | "quotas"] => Some("GET,HEAD,POST"),
        ["admin", "login-flows", "start" | "complete" | "cookie"] => Some("POST"),
        ["admin", "login-flows", "device", "start" | "poll"] => Some("POST"),
        ["admin", "update", "check" | "notes" | "status"] => Some("GET,HEAD"),
        ["admin", "update", "apply"] => Some("POST"),
        ["admin", "connectivity", "test"] => Some("POST"),
        ["admin", "orgs", _, "teams"]
        | [
            "admin",
            "providers",
            _,
            "models" | "credentials" | "routing-rules" | "rule-sets",
        ]
        | ["admin", "routes", _, "members"]
        | ["admin", "rule-sets", _, "rules"]
        | ["admin", "users", _, "keys"] => Some("GET,HEAD,POST"),
        ["admin", "credentials", _, "tasks"] => Some("GET,HEAD,POST"),
        ["admin", "credentials", _, "tasks", _]
        | [
            "admin",
            "credentials",
            _,
            "tasks",
            _,
            "turns",
            _,
            "siblings",
        ] => Some("GET,HEAD"),
        // Health snapshots double as operator resets (DELETE).
        ["admin", "credentials", _, "status" | "model-statuses"] => Some("GET,HEAD,DELETE"),
        ["admin", "credentials", _, "usage-summary"] => Some("GET,HEAD"),
        ["admin", "providers", _, "upstream-models"]
        | [
            "admin",
            "credentials",
            _,
            "secret" | "usage" | "rate-limit-reset-credits" | "account" | "profile" | "settings",
        ]
        | ["admin", "logs", _, "downstream" | "upstream"] => Some("GET,HEAD"),
        ["admin", "credentials", _, "rate-limit-reset-credit"]
        | [
            "admin",
            "credentials",
            _,
            "rate-limit-reset-credits",
            "consume",
        ]
        | ["admin", "providers", _, "routing-rules", "reset"] => Some("POST"),
        ["admin", "providers", _, "credentials", "import"] => Some("POST"),
        ["admin", "providers", _, "credentials", _] => Some("GET,HEAD"),
        [
            "user",
            "me" | "usage" | "usage-rollups" | "quota" | "rate-limits" | "route-permissions",
        ] => Some("GET,HEAD"),
        ["user", "keys"] => Some("GET,HEAD,POST"),
        ["user", "keys", _] => Some("PATCH,DELETE"),
        ["user", "change-password"] => Some("POST"),
        _ => None,
    }
}

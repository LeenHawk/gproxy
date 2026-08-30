use super::{ColumnKind::*, ColumnSpec as Col, IndexSpec, SchemaVersion, TableSpec};

pub const TABLES: &[TableSpec] = &[
    TableSpec {
        version: SchemaVersion::Control,
        name: "providers",
        columns: &[
            Col::id(),
            Col::required("name", Text).unique(),
            Col::required("channel", Text),
            Col::required("settings_json", Text),
            Col::required("enabled", Integer),
            Col::optional("tls_fingerprint", Text).since(SchemaVersion::Admin),
            Col::optional("label", Text).since(SchemaVersion::Configuration),
            Col::required("credential_strategy", Text)
                .default("'round_robin'")
                .since(SchemaVersion::Configuration),
            Col::optional("proxy_url", Text).since(SchemaVersion::Configuration),
        ],
        indexes: &[],
    },
    TableSpec {
        version: SchemaVersion::Control,
        name: "credentials",
        columns: &[
            Col::id(),
            Col::required("provider_id", Integer),
            Col::optional("label", Text),
            Col::required("ciphertext", Blob),
            Col::required("wrapped_key", Blob),
            Col::required("payload_nonce", Blob),
            Col::required("key_nonce", Blob),
            Col::required("version", Integer),
            Col::required("enabled", Integer),
            Col::required("weight", Integer)
                .default("100")
                .since(SchemaVersion::Routing),
            Col::optional("rpm_limit", Integer).since(SchemaVersion::Routing),
            Col::optional("tpm_limit", Integer).since(SchemaVersion::Routing),
            Col::optional("proxy_url", Text).since(SchemaVersion::Routing),
            Col::optional("tls_fingerprint", Text).since(SchemaVersion::Routing),
            Col::required("kind", Text)
                .default("'api_key'")
                .since(SchemaVersion::Configuration),
        ],
        indexes: &[IndexSpec {
            name: "ix_credentials_provider_enabled",
            columns: &["provider_id", "enabled", "id"],
            unique: false,
            added_in: None,
        }],
    },
    TableSpec {
        version: SchemaVersion::Control,
        name: "routes",
        columns: &[
            Col::id(),
            Col::required("name", Text).unique(),
            Col::required("max_attempts", Integer),
            Col::required("enabled", Integer),
        ],
        indexes: &[],
    },
    TableSpec {
        version: SchemaVersion::Control,
        name: "route_members",
        columns: &[
            Col::id(),
            Col::required("route_id", Integer),
            Col::required("provider_id", Integer),
            Col::optional("credential_id", Integer),
            Col::required("upstream_model", Text),
            Col::required("priority", Integer),
            Col::required("tier", Integer)
                .default("0")
                .since(SchemaVersion::Routing),
            Col::required("weight", Integer)
                .default("100")
                .since(SchemaVersion::Routing),
            Col::required("enabled", Integer),
        ],
        indexes: &[
            IndexSpec {
                name: "ix_route_members_route_order",
                columns: &["route_id", "enabled", "priority", "id"],
                unique: false,
                added_in: None,
            },
            IndexSpec {
                name: "ix_route_members_route_balance",
                columns: &["route_id", "enabled", "tier", "weight", "id"],
                unique: false,
                added_in: Some(SchemaVersion::Routing),
            },
            IndexSpec {
                name: "ix_route_members_provider",
                columns: &["provider_id", "credential_id"],
                unique: false,
                added_in: None,
            },
        ],
    },
    TableSpec {
        version: SchemaVersion::Control,
        name: "aliases",
        columns: &[
            Col::id(),
            Col::required("alias", Text),
            Col::required("target", Text),
            Col::optional("provider_id", Integer),
            Col::required("priority", Integer),
            Col::required("enabled", Integer),
        ],
        indexes: &[IndexSpec {
            name: "ix_aliases_scope_order",
            columns: &["provider_id", "enabled", "priority", "id"],
            unique: false,
            added_in: None,
        }],
    },
    TableSpec {
        version: SchemaVersion::Control,
        // What a model can do is a property of the provider serving it. A route with
        // several members advertises the conservative fold of these rows, never a
        // hand-typed number that can drift from what the upstreams actually accept.
        name: "provider_models",
        columns: &[
            Col::id(),
            Col::required("provider_id", Integer),
            Col::required("model_id", Text),
            Col::optional("display_name", Text),
            Col::optional("variants_json", Text),
            Col::optional("context_window", Integer),
            Col::optional("max_output_tokens", Integer),
            Col::optional("thinking_supported", Integer),
            Col::optional("thinking_adaptive_supported", Integer),
            Col::optional("thinking_enabled_supported", Integer),
            Col::required("enabled", Integer),
        ],
        indexes: &[IndexSpec {
            name: "uq_provider_models_pair",
            columns: &["provider_id", "model_id"],
            unique: true,
            added_in: None,
        }],
    },
    TableSpec {
        version: SchemaVersion::Control,
        // The exposed model is the client-facing name and nothing more. What a model
        // can do is a property of the provider serving it, so it lives on provider_models
        // and the catalogue folds it across a route's members.
        name: "exposed_models",
        columns: &[
            Col::id(),
            Col::required("name", Text).unique(),
            Col::required("route_id", Integer),
            Col::required("enabled", Integer),
        ],
        indexes: &[IndexSpec {
            name: "ix_exposed_models_route",
            columns: &["route_id", "enabled"],
            unique: false,
            added_in: None,
        }],
    },
    TableSpec {
        version: SchemaVersion::Control,
        name: "price_rules",
        columns: &[
            Col::id(),
            Col::optional("provider_id", Integer),
            Col::required("model_pattern", Text),
            Col::optional("tiers_json", Text).since(SchemaVersion::Pricing),
            Col::required("priority", Integer),
            Col::required("enabled", Integer),
        ],
        indexes: &[IndexSpec {
            name: "ix_price_rules_resolve",
            columns: &["provider_id", "enabled", "priority", "id"],
            unique: false,
            added_in: None,
        }],
    },
    TableSpec {
        version: SchemaVersion::Control,
        name: "price_rates",
        columns: &[
            Col::id(),
            Col::required("rule_id", Integer),
            Col::required("metric", Text),
            Col::required("unit_size", Integer),
            Col::required("price", Text),
            Col::optional("conditions_json", Text),
            Col::required("priority", Integer),
        ],
        indexes: &[IndexSpec {
            name: "ix_price_rates_rule_metric",
            columns: &["rule_id", "metric", "priority", "id"],
            unique: false,
            added_in: None,
        }],
    },
    TableSpec {
        version: SchemaVersion::Control,
        name: "settings",
        columns: &[
            Col::required("key", Text).primary(),
            Col::required("value_json", Text),
        ],
        indexes: &[],
    },
    TableSpec {
        version: SchemaVersion::Process,
        name: "routing_rules",
        columns: &[
            Col::id(),
            Col::required("provider_id", Integer),
            Col::required("operation", Text),
            Col::required("kind", Text),
            Col::required("implementation", Text),
            Col::optional("dest_operation", Text),
            Col::optional("dest_kind", Text),
            Col::required("sort_order", Integer),
            Col::required("enabled", Integer),
            Col::required("origin", Text)
                .default("'operator'")
                .since(SchemaVersion::Wave26),
            Col::required("created_at", Integer),
            Col::required("updated_at", Integer),
        ],
        indexes: &[
            IndexSpec {
                name: "uq_routing_rules_provider_key",
                columns: &["provider_id", "operation", "kind"],
                unique: true,
                added_in: None,
            },
            IndexSpec {
                name: "ix_routing_rules_provider_order",
                columns: &["provider_id", "enabled", "sort_order", "id"],
                unique: false,
                added_in: None,
            },
        ],
    },
    TableSpec {
        version: SchemaVersion::Process,
        name: "rule_sets",
        columns: &[
            Col::id(),
            Col::required("name", Text).unique(),
            Col::optional("description", Text),
            Col::required("enabled", Integer),
            Col::required("created_at", Integer),
            Col::required("updated_at", Integer),
        ],
        indexes: &[],
    },
    TableSpec {
        version: SchemaVersion::Process,
        name: "rules",
        columns: &[
            Col::id(),
            Col::required("rule_set_id", Integer),
            Col::required("kind", Text),
            Col::required("config_json", Text),
            Col::optional("filter_model_pattern", Text),
            Col::optional("filter_operations_json", Text),
            Col::optional("filter_header_pattern", Text),
            Col::required("sort_order", Integer),
            Col::required("enabled", Integer),
            Col::required("created_at", Integer),
            Col::required("updated_at", Integer),
        ],
        indexes: &[IndexSpec {
            name: "ix_rules_set_order",
            columns: &["rule_set_id", "enabled", "sort_order", "id"],
            unique: false,
            added_in: None,
        }],
    },
    TableSpec {
        version: SchemaVersion::Process,
        name: "provider_rule_sets",
        columns: &[
            Col::id(),
            Col::required("provider_id", Integer),
            Col::required("rule_set_id", Integer),
            Col::required("sort_order", Integer),
            Col::required("enabled", Integer),
            Col::required("origin", Text)
                .default("'operator'")
                .since(SchemaVersion::Wave26),
            Col::required("created_at", Integer),
            Col::required("updated_at", Integer),
        ],
        indexes: &[
            IndexSpec {
                name: "uq_provider_rule_sets_pair",
                columns: &["provider_id", "rule_set_id"],
                unique: true,
                added_in: None,
            },
            IndexSpec {
                name: "ix_provider_rule_sets_provider_order",
                columns: &["provider_id", "enabled", "sort_order", "id"],
                unique: false,
                added_in: None,
            },
        ],
    },
];

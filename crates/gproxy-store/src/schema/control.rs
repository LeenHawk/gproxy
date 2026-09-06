use super::{ColumnKind::*, ColumnSpec as Col, IndexSpec, Ownership, SchemaVersion, TableSpec};

pub const TABLES: &[TableSpec] = &[
    TableSpec {
        version: SchemaVersion::Initial,
        name: "providers",
        columns: &[
            Col::id(),
            Col::required("name", Text).unique(),
            Col::required("channel", Text),
            Col::required("settings_json", Text),
            Col::required("enabled", Integer),
            Col::optional("tls_fingerprint", Text),
            Col::optional("label", Text),
            Col::required("credential_strategy", Text).default("'round_robin'"),
            Col::optional("proxy_url", Text),
        ],
        owns: &[
            Ownership::Owns {
                table: "credentials",
                column: "provider_id",
            },
            Ownership::Owns {
                table: "route_members",
                column: "provider_id",
            },
            Ownership::Owns {
                table: "aliases",
                column: "provider_id",
            },
            Ownership::Owns {
                table: "provider_models",
                column: "provider_id",
            },
            Ownership::Owns {
                table: "provider_model_modalities",
                column: "provider_id",
            },
            Ownership::Owns {
                table: "provider_model_parameters",
                column: "provider_id",
            },
            Ownership::Owns {
                table: "provider_model_reasoning_levels",
                column: "provider_id",
            },
            Ownership::Owns {
                table: "provider_model_service_tiers",
                column: "provider_id",
            },
            Ownership::Owns {
                table: "provider_model_methods",
                column: "provider_id",
            },
            Ownership::Owns {
                table: "price_rules",
                column: "provider_id",
            },
            Ownership::Owns {
                table: "routing_rules",
                column: "provider_id",
            },
            Ownership::Owns {
                table: "provider_rule_sets",
                column: "provider_id",
            },
            Ownership::Owns {
                table: "permissions",
                column: "provider_id",
            },
            Ownership::Owns {
                table: "surface_bindings",
                column: "provider_id",
            },
            Ownership::Detaches {
                table: "oauth_grants",
                column: "provider_id",
            },
            Ownership::Detaches {
                table: "oauth_devices",
                column: "provider_id",
            },
        ],
        indexes: &[],
    },
    TableSpec {
        version: SchemaVersion::Initial,
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
            Col::required("weight", Integer).default("100"),
            Col::optional("rpm_limit", Integer),
            Col::optional("tpm_limit", Integer),
            Col::optional("proxy_url", Text),
            Col::optional("tls_fingerprint", Text),
            Col::required("kind", Text).default("'api_key'"),
        ],
        owns: &[
            Ownership::Owns {
                table: "credential_health",
                column: "credential_id",
            },
            Ownership::Owns {
                table: "credential_quota_activity",
                column: "credential_id",
            },
            Ownership::Owns {
                table: "surface_bindings",
                column: "credential_id",
            },
            Ownership::Scoped {
                table: "quotas",
                kind: "credential",
            },
        ],
        indexes: &[IndexSpec {
            name: "ix_credentials_provider_enabled",
            columns: &["provider_id", "enabled", "id"],
            unique: false,
            added_in: None,
        }],
    },
    TableSpec {
        version: SchemaVersion::Initial,
        name: "routes",
        columns: &[
            Col::id(),
            Col::required("name", Text).unique(),
            Col::required("max_attempts", Integer),
            Col::required("enabled", Integer),
        ],
        owns: &[
            Ownership::Owns {
                table: "route_members",
                column: "route_id",
            },
            Ownership::Owns {
                table: "exposed_models",
                column: "route_id",
            },
        ],
        indexes: &[],
    },
    TableSpec {
        version: SchemaVersion::Initial,
        name: "route_members",
        columns: &[
            Col::id(),
            Col::required("route_id", Integer),
            Col::required("provider_id", Integer),
            Col::optional("credential_id", Integer),
            Col::required("upstream_model", Text),
            Col::required("priority", Integer),
            Col::required("tier", Integer).default("0"),
            Col::required("weight", Integer).default("100"),
            Col::required("enabled", Integer),
        ],
        owns: &[],
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
                added_in: None,
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
        version: SchemaVersion::Initial,
        name: "aliases",
        columns: &[
            Col::id(),
            Col::required("alias", Text),
            Col::required("target", Text),
            Col::optional("provider_id", Integer),
            Col::required("priority", Integer),
            Col::required("enabled", Integer),
        ],
        owns: &[],
        indexes: &[IndexSpec {
            name: "ix_aliases_scope_order",
            columns: &["provider_id", "enabled", "priority", "id"],
            unique: false,
            added_in: None,
        }],
    },
    TableSpec {
        version: SchemaVersion::Initial,
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
            Col::optional("description", Text).since(SchemaVersion::ModelMetadata),
            Col::optional("instructions", Text).since(SchemaVersion::ModelMetadata),
            Col::optional("max_context_window", Integer).since(SchemaVersion::ModelMetadata),
            Col::optional("default_reasoning_level", Text).since(SchemaVersion::ModelMetadata),
            Col::optional("default_service_tier", Text).since(SchemaVersion::ModelMetadata),
            Col::optional("shell_type", Text).since(SchemaVersion::ModelMetadata),
            Col::optional("support_verbosity", Integer).since(SchemaVersion::ModelMetadata),
            Col::optional("default_verbosity", Text).since(SchemaVersion::ModelMetadata),
            Col::optional("reasoning_summary_supported", Integer)
                .since(SchemaVersion::ModelMetadata),
            Col::optional("default_reasoning_summary", Text).since(SchemaVersion::ModelMetadata),
            Col::optional("apply_patch_tool_type", Text).since(SchemaVersion::ModelMetadata),
            Col::optional("web_search_tool_type", Text).since(SchemaVersion::ModelMetadata),
            Col::optional("truncation_mode", Text).since(SchemaVersion::ModelMetadata),
            Col::optional("truncation_limit", Integer).since(SchemaVersion::ModelMetadata),
            Col::optional("auto_compact_token_limit", Integer).since(SchemaVersion::ModelMetadata),
            Col::optional("effective_context_window_percent", Integer)
                .since(SchemaVersion::ModelMetadata),
            Col::optional("batch_supported", Integer).since(SchemaVersion::ModelMetadata),
            Col::optional("citations_supported", Integer).since(SchemaVersion::ModelMetadata),
            Col::optional("code_execution_supported", Integer).since(SchemaVersion::ModelMetadata),
            Col::optional("context_management_supported", Integer)
                .since(SchemaVersion::ModelMetadata),
            Col::optional("structured_outputs_supported", Integer)
                .since(SchemaVersion::ModelMetadata),
            Col::optional("pdf_input_supported", Integer).since(SchemaVersion::ModelMetadata),
            Col::optional("image_detail_original_supported", Integer)
                .since(SchemaVersion::ModelMetadata),
            Col::optional("search_supported", Integer).since(SchemaVersion::ModelMetadata),
            Col::required("input_modalities_known", Integer)
                .default("0")
                .since(SchemaVersion::ModelMetadata),
            Col::required("output_modalities_known", Integer)
                .default("0")
                .since(SchemaVersion::ModelMetadata),
            Col::required("parameters_known", Integer)
                .default("0")
                .since(SchemaVersion::ModelMetadata),
            Col::required("reasoning_levels_known", Integer)
                .default("0")
                .since(SchemaVersion::ModelMetadata),
            Col::required("service_tiers_known", Integer)
                .default("0")
                .since(SchemaVersion::ModelMetadata),
            Col::required("generation_methods_known", Integer)
                .default("0")
                .since(SchemaVersion::ModelMetadata),
            Col::required("supported_actions_known", Integer)
                .default("0")
                .since(SchemaVersion::ModelMetadata),
            Col::required("enabled", Integer),
        ],
        owns: &[],
        indexes: &[IndexSpec {
            name: "uq_provider_models_pair",
            columns: &["provider_id", "model_id"],
            unique: true,
            added_in: None,
        }],
    },
    TableSpec {
        version: SchemaVersion::Initial,
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
        owns: &[],
        indexes: &[IndexSpec {
            name: "ix_exposed_models_route",
            columns: &["route_id", "enabled"],
            unique: false,
            added_in: None,
        }],
    },
    TableSpec {
        version: SchemaVersion::Initial,
        name: "price_rules",
        columns: &[
            Col::id(),
            Col::optional("provider_id", Integer),
            Col::required("model_pattern", Text),
            Col::optional("tiers_json", Text),
            Col::required("priority", Integer),
            Col::required("enabled", Integer),
        ],
        owns: &[Ownership::Owns {
            table: "price_rates",
            column: "rule_id",
        }],
        indexes: &[IndexSpec {
            name: "ix_price_rules_resolve",
            columns: &["provider_id", "enabled", "priority", "id"],
            unique: false,
            added_in: None,
        }],
    },
    TableSpec {
        version: SchemaVersion::Initial,
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
        owns: &[],
        indexes: &[IndexSpec {
            name: "ix_price_rates_rule_metric",
            columns: &["rule_id", "metric", "priority", "id"],
            unique: false,
            added_in: None,
        }],
    },
    TableSpec {
        version: SchemaVersion::Initial,
        name: "settings",
        columns: &[
            Col::required("key", Text).primary(),
            Col::required("value_json", Text),
        ],
        owns: &[],
        indexes: &[],
    },
    TableSpec {
        version: SchemaVersion::Initial,
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
            Col::required("origin", Text).default("'operator'"),
            Col::required("created_at", Integer),
            Col::required("updated_at", Integer),
        ],
        owns: &[],
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
        version: SchemaVersion::Initial,
        name: "rule_sets",
        columns: &[
            Col::id(),
            Col::required("name", Text).unique(),
            Col::optional("description", Text),
            Col::required("enabled", Integer),
            Col::required("created_at", Integer),
            Col::required("updated_at", Integer),
        ],
        owns: &[
            Ownership::Owns {
                table: "rules",
                column: "rule_set_id",
            },
            Ownership::Owns {
                table: "provider_rule_sets",
                column: "rule_set_id",
            },
        ],
        indexes: &[],
    },
    TableSpec {
        version: SchemaVersion::Initial,
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
        owns: &[],
        indexes: &[IndexSpec {
            name: "ix_rules_set_order",
            columns: &["rule_set_id", "enabled", "sort_order", "id"],
            unique: false,
            added_in: None,
        }],
    },
    TableSpec {
        version: SchemaVersion::Initial,
        name: "provider_rule_sets",
        columns: &[
            Col::id(),
            Col::required("provider_id", Integer),
            Col::required("rule_set_id", Integer),
            Col::required("sort_order", Integer),
            Col::required("enabled", Integer),
            Col::required("origin", Text).default("'operator'"),
            Col::required("created_at", Integer),
            Col::required("updated_at", Integer),
        ],
        owns: &[],
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

use super::{ColumnKind::*, ColumnSpec as Col, IndexSpec, Ownership, SchemaVersion, TableSpec};

pub const TABLES: &[TableSpec] = &[
    TableSpec {
        version: SchemaVersion::Initial,
        name: "organizations",
        columns: &[
            Col::id(),
            Col::required("name", Text).unique(),
            Col::required("enabled", Integer),
        ],
        owns: &[
            Ownership::Owns {
                table: "teams",
                column: "organization_id",
            },
            Ownership::Owns {
                table: "users",
                column: "organization_id",
            },
            Ownership::Scoped {
                table: "permissions",
                kind: "organization",
            },
            Ownership::Scoped {
                table: "rate_limits",
                kind: "organization",
            },
            Ownership::Scoped {
                table: "quotas",
                kind: "organization",
            },
        ],
        indexes: &[],
    },
    TableSpec {
        version: SchemaVersion::Initial,
        name: "teams",
        columns: &[
            Col::id(),
            Col::required("organization_id", Integer),
            Col::required("name", Text),
            Col::required("enabled", Integer),
        ],
        owns: &[
            Ownership::Detaches {
                table: "users",
                column: "team_id",
            },
            Ownership::Scoped {
                table: "permissions",
                kind: "team",
            },
            Ownership::Scoped {
                table: "rate_limits",
                kind: "team",
            },
            Ownership::Scoped {
                table: "quotas",
                kind: "team",
            },
        ],
        indexes: &[IndexSpec {
            name: "uq_teams_organization_name",
            columns: &["organization_id", "name"],
            unique: true,
            added_in: None,
        }],
    },
    TableSpec {
        version: SchemaVersion::Initial,
        name: "users",
        columns: &[
            Col::id(),
            Col::required("name", Text).unique(),
            Col::optional("organization_id", Integer),
            Col::optional("team_id", Integer),
            Col::optional("password_hash", Text),
            Col::required("enabled", Integer),
            Col::required("is_admin", Integer).default("0"),
        ],
        owns: &[
            Ownership::Owns {
                table: "user_keys",
                column: "user_id",
            },
            Ownership::Owns {
                table: "user_sessions",
                column: "user_id",
            },
            Ownership::Owns {
                table: "oauth_grants",
                column: "user_id",
            },
            Ownership::Owns {
                table: "surface_bindings",
                column: "owner_user_id",
            },
            Ownership::Scoped {
                table: "permissions",
                kind: "user",
            },
            Ownership::Scoped {
                table: "rate_limits",
                kind: "user",
            },
            Ownership::Scoped {
                table: "quotas",
                kind: "user",
            },
        ],
        indexes: &[
            IndexSpec {
                name: "ix_users_organization",
                columns: &["organization_id", "enabled"],
                unique: false,
                added_in: None,
            },
            IndexSpec {
                name: "ix_users_team",
                columns: &["team_id", "enabled"],
                unique: false,
                added_in: None,
            },
        ],
    },
    TableSpec {
        version: SchemaVersion::Initial,
        name: "user_keys",
        columns: &[
            Col::id(),
            Col::required("user_id", Integer),
            Col::required("digest", Blob).unique(),
            Col::optional("label", Text),
            Col::optional("expires_at", Integer),
            Col::required("enabled", Integer),
            Col::required("digest_version", Integer).default("1"),
            Col::optional("prefix", Text),
            Col::optional("ciphertext", Blob),
            Col::optional("wrapped_key", Blob),
            Col::optional("payload_nonce", Blob),
            Col::optional("key_nonce", Blob),
        ],
        owns: &[
            Ownership::Owns {
                table: "oauth_grants",
                column: "user_key_id",
            },
            Ownership::Scoped {
                table: "permissions",
                kind: "user_key",
            },
            Ownership::Scoped {
                table: "rate_limits",
                kind: "user_key",
            },
            Ownership::Scoped {
                table: "quotas",
                kind: "user_key",
            },
        ],
        indexes: &[IndexSpec {
            name: "ix_user_keys_user_enabled",
            columns: &["user_id", "enabled"],
            unique: false,
            added_in: None,
        }],
    },
    TableSpec {
        version: SchemaVersion::Initial,
        name: "user_sessions",
        columns: &[
            Col::id(),
            Col::required("token_digest", Blob).unique(),
            Col::required("user_id", Integer),
            Col::required("created_at", Integer),
            Col::required("expires_at", Integer),
        ],
        owns: &[],
        indexes: &[IndexSpec {
            name: "ix_user_sessions_expiry",
            columns: &["expires_at", "id"],
            unique: false,
            added_in: None,
        }],
    },
    TableSpec {
        version: SchemaVersion::Initial,
        name: "permissions",
        columns: &[
            Col::id(),
            Col::required("subject_kind", Text),
            Col::required("subject_id", Integer),
            Col::optional("provider_id", Integer),
            Col::optional("operation_group", Text),
            Col::required("allowed", Integer),
        ],
        owns: &[],
        indexes: &[IndexSpec {
            name: "ix_permissions_subject",
            columns: &["subject_kind", "subject_id", "provider_id"],
            unique: false,
            added_in: None,
        }],
    },
    TableSpec {
        version: SchemaVersion::Initial,
        name: "rate_limits",
        columns: &[
            Col::id(),
            Col::required("subject_kind", Text),
            Col::required("subject_id", Integer),
            Col::required("requests", Integer),
            Col::required("window_seconds", Integer),
        ],
        owns: &[],
        indexes: &[IndexSpec {
            name: "uq_rate_limits_subject_window",
            columns: &["subject_kind", "subject_id", "window_seconds"],
            unique: true,
            added_in: None,
        }],
    },
    TableSpec {
        version: SchemaVersion::Initial,
        name: "quotas",
        columns: &[
            Col::id(),
            Col::required("subject_kind", Text),
            Col::required("subject_id", Integer),
            Col::required("quota_total", Text).nullable_since(SchemaVersion::CredentialBudgets),
            Col::optional("quota_daily", Text),
            Col::optional("quota_weekly", Text),
            Col::optional("quota_monthly", Text),
            Col::optional("quota_5h", Text),
            Col::optional("quota_7d", Text),
            Col::required("enabled", Integer).default("1"),
        ],
        owns: &[Ownership::Owns {
            table: "quota_windows",
            column: "quota_id",
        }],
        indexes: &[IndexSpec {
            name: "uq_quotas_subject",
            columns: &["subject_kind", "subject_id"],
            unique: true,
            added_in: None,
        }],
    },
];

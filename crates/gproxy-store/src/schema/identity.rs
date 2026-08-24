use super::{ColumnKind::*, ColumnSpec as Col, IndexSpec, SchemaVersion, TableSpec};

pub const TABLES: &[TableSpec] = &[
    TableSpec {
        version: SchemaVersion::Control,
        name: "organizations",
        columns: &[
            Col::id(),
            Col::required("name", Text).unique(),
            Col::required("enabled", Integer),
        ],
        indexes: &[],
    },
    TableSpec {
        version: SchemaVersion::Control,
        name: "teams",
        columns: &[
            Col::id(),
            Col::required("organization_id", Integer),
            Col::required("name", Text),
            Col::required("enabled", Integer),
        ],
        indexes: &[IndexSpec {
            name: "uq_teams_organization_name",
            columns: &["organization_id", "name"],
            unique: true,
        }],
    },
    TableSpec {
        version: SchemaVersion::Control,
        name: "users",
        columns: &[
            Col::id(),
            Col::required("name", Text).unique(),
            Col::optional("organization_id", Integer),
            Col::optional("team_id", Integer),
            Col::required("enabled", Integer),
        ],
        indexes: &[
            IndexSpec {
                name: "ix_users_organization",
                columns: &["organization_id", "enabled"],
                unique: false,
            },
            IndexSpec {
                name: "ix_users_team",
                columns: &["team_id", "enabled"],
                unique: false,
            },
        ],
    },
    TableSpec {
        version: SchemaVersion::Control,
        name: "user_keys",
        columns: &[
            Col::id(),
            Col::required("user_id", Integer),
            Col::required("digest", Blob).unique(),
            Col::optional("label", Text),
            Col::optional("expires_at", Integer),
            Col::required("enabled", Integer),
        ],
        indexes: &[IndexSpec {
            name: "ix_user_keys_user_enabled",
            columns: &["user_id", "enabled"],
            unique: false,
        }],
    },
    TableSpec {
        version: SchemaVersion::Control,
        name: "permissions",
        columns: &[
            Col::id(),
            Col::required("subject_kind", Text),
            Col::required("subject_id", Integer),
            Col::optional("provider_id", Integer),
            Col::optional("operation_group", Text),
            Col::required("allowed", Integer),
        ],
        indexes: &[IndexSpec {
            name: "ix_permissions_subject",
            columns: &["subject_kind", "subject_id", "provider_id"],
            unique: false,
        }],
    },
    TableSpec {
        version: SchemaVersion::Control,
        name: "rate_limits",
        columns: &[
            Col::id(),
            Col::required("subject_kind", Text),
            Col::required("subject_id", Integer),
            Col::required("requests", Integer),
            Col::required("window_seconds", Integer),
        ],
        indexes: &[IndexSpec {
            name: "uq_rate_limits_subject_window",
            columns: &["subject_kind", "subject_id", "window_seconds"],
            unique: true,
        }],
    },
    TableSpec {
        version: SchemaVersion::Control,
        name: "quotas",
        columns: &[
            Col::id(),
            Col::required("subject_kind", Text),
            Col::required("subject_id", Integer),
            Col::required("token_limit", Integer),
            Col::required("window_seconds", Integer),
        ],
        indexes: &[IndexSpec {
            name: "uq_quotas_subject_window",
            columns: &["subject_kind", "subject_id", "window_seconds"],
            unique: true,
        }],
    },
];

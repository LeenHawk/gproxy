use super::{ColumnKind::*, ColumnSpec as Col, IndexSpec, SchemaVersion, TableSpec};

pub const TABLES: &[TableSpec] = &[
    TableSpec {
        version: SchemaVersion::Wave26,
        name: "admin_audit_events",
        columns: &[
            Col::id(),
            Col::required("actor_user_id", Integer),
            Col::required("action", Text),
            Col::required("target_kind", Text),
            Col::optional("target_id", Integer),
            Col::required("at", Integer),
            Col::optional("details_json", Text),
        ],
        indexes: &[IndexSpec {
            name: "ix_admin_audit_at",
            columns: &["at", "id"],
            unique: false,
            added_in: None,
        }],
    },
    TableSpec {
        version: SchemaVersion::Wave26,
        name: "credential_health",
        columns: &[
            Col::id(),
            Col::required("credential_id", Integer),
            Col::required("model", Text),
            Col::required("credential_version", Integer),
            Col::required("version", Integer),
            Col::required("state", Text),
            Col::required("observed_at", Integer),
            Col::optional("response_status", Integer),
            Col::optional("detail", Text),
        ],
        indexes: &[
            IndexSpec {
                name: "uq_credential_health_model",
                columns: &["credential_id", "model"],
                unique: true,
                added_in: None,
            },
            IndexSpec {
                name: "ix_credential_health_state",
                columns: &["state", "observed_at", "credential_id", "model"],
                unique: false,
                added_in: None,
            },
        ],
    },
];

pub(super) const LEGACY_TABLES: &[TableSpec] = &[
    TableSpec {
        version: SchemaVersion::Admin,
        name: "admin_accounts",
        columns: &[
            Col::id(),
            Col::required("username", Text).unique(),
            Col::required("password_hash", Text),
            Col::required("enabled", Integer),
            Col::required("created_at", Integer),
        ],
        indexes: &[],
    },
    TableSpec {
        version: SchemaVersion::Admin,
        name: "admin_sessions",
        columns: &[
            Col::id(),
            Col::required("token_digest", Blob).unique(),
            Col::required("admin_id", Integer),
            Col::required("created_at", Integer),
            Col::required("expires_at", Integer),
        ],
        indexes: &[IndexSpec {
            name: "ix_admin_sessions_expiry",
            columns: &["expires_at", "id"],
            unique: false,
            added_in: None,
        }],
    },
    TableSpec {
        version: SchemaVersion::Configuration,
        name: "admin_api_keys",
        columns: &[
            Col::required("digest", Blob).primary(),
            Col::required("admin_id", Integer),
            Col::required("created_at", Integer),
        ],
        indexes: &[],
    },
    TableSpec {
        version: SchemaVersion::Admin,
        name: "admin_audit_events",
        columns: &[
            Col::id(),
            Col::required("actor_admin_id", Integer),
            Col::required("action", Text),
            Col::required("target_kind", Text),
            Col::optional("target_id", Integer),
            Col::required("at", Integer),
            Col::optional("details_json", Text),
        ],
        indexes: &[IndexSpec {
            name: "ix_admin_audit_at",
            columns: &["at", "id"],
            unique: false,
            added_in: None,
        }],
    },
    TableSpec {
        version: SchemaVersion::Admin,
        name: "credential_health",
        columns: &[
            Col::required("credential_id", Integer).primary(),
            Col::required("credential_version", Integer),
            Col::required("version", Integer),
            Col::required("state", Text),
            Col::required("observed_at", Integer),
            Col::optional("response_status", Integer),
            Col::optional("detail", Text),
        ],
        indexes: &[IndexSpec {
            name: "ix_credential_health_state",
            columns: &["state", "observed_at", "credential_id"],
            unique: false,
            added_in: None,
        }],
    },
];

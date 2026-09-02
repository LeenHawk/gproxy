use super::{ColumnKind::*, ColumnSpec as Col, IndexSpec, SchemaVersion, TableSpec};

pub const TABLES: &[TableSpec] = &[
    TableSpec {
        version: SchemaVersion::Initial,
        name: "admin_audit_events",
        columns: &[
            Col::id(),
            Col::required("actor_user_id", Integer),
            Col::required("action", Text),
            Col::required("target_kind", Text),
            Col::optional("target_id", Integer),
            Col::required("at", Integer),
            Col::optional("details_json", Text),
            Col::optional("client_ip", Text),
        ],
        indexes: &[IndexSpec {
            name: "ix_admin_audit_at",
            columns: &["at", "id"],
            unique: false,
            added_in: None,
        }],
    },
    TableSpec {
        version: SchemaVersion::Initial,
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

use super::super::{ColumnKind::*, ColumnSpec as Col, IndexSpec, SchemaVersion, TableSpec};

pub(super) const TABLES: &[TableSpec] = &[
    TableSpec {
        version: SchemaVersion::Runtime,
        name: "request_logs",
        columns: &[
            Col::id(),
            Col::required("request_id", Text).unique(),
            Col::required("at", Integer),
            Col::required("method", Text),
            Col::required("path", Text),
            Col::optional("query", Text),
            Col::optional("request_headers", Text).since(SchemaVersion::Logging),
            Col::optional("request_body", Blob).since(SchemaVersion::Logging),
            Col::optional("response_status", Integer),
            Col::optional("error_kind", Text),
            Col::optional("response_headers", Text).since(SchemaVersion::Logging),
            Col::optional("response_body", Blob).since(SchemaVersion::Logging),
        ],
        indexes: &[IndexSpec {
            name: "ix_request_logs_at",
            columns: &["at", "id"],
            unique: false,
            added_in: None,
        }],
    },
    TableSpec {
        version: SchemaVersion::Runtime,
        name: "wire_logs",
        columns: &[
            Col::id(),
            Col::required("request_id", Text),
            Col::required("at", Integer),
            Col::optional("provider_id", Integer),
            Col::optional("credential_id", Integer),
            Col::optional("upstream_url", Text),
            Col::optional("request_method", Text).since(SchemaVersion::Logging),
            Col::optional("request_headers", Text).since(SchemaVersion::Logging),
            Col::optional("response_status", Integer),
            Col::optional("response_headers", Text).since(SchemaVersion::Logging),
            Col::optional("request_body", Blob),
            Col::optional("response_body", Blob),
        ],
        indexes: &[IndexSpec {
            name: "ix_wire_logs_request",
            columns: &["request_id", "id"],
            unique: false,
            added_in: None,
        }],
    },
];

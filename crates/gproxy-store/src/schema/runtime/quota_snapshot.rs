use super::super::{ColumnKind::*, ColumnSpec as Col, IndexSpec, SchemaVersion, TableSpec};

pub(super) const TABLES: &[TableSpec] = &[
    TableSpec {
        version: SchemaVersion::QuotaSnapshots,
        name: "credential_quota_sources",
        columns: &[
            Col::required("credential_id", Integer),
            Col::required("source_id", Text),
            Col::required("capability_json", Text),
            Col::optional("attempted_at_ms", Integer),
            Col::optional("observed_at_ms", Integer),
            Col::optional("error_code", Text),
            Col::optional("error_message", Text),
            Col::required("entries_json", Text),
            Col::optional("reset_credits_json", Text),
        ],
        owns: &[],
        indexes: &[IndexSpec {
            name: "uq_credential_quota_source",
            columns: &["credential_id", "source_id"],
            unique: true,
            added_in: None,
        }],
    },
    TableSpec {
        version: SchemaVersion::QuotaSnapshots,
        name: "credential_quota_response_entries",
        columns: &[
            Col::required("credential_id", Integer),
            Col::required("source_id", Text),
            Col::required("entry_id", Text),
            Col::required("observed_at_ms", Integer),
            Col::required("entry_json", Text),
        ],
        owns: &[],
        indexes: &[IndexSpec {
            name: "uq_credential_quota_response_entry",
            columns: &["credential_id", "source_id", "entry_id"],
            unique: true,
            added_in: None,
        }],
    },
];

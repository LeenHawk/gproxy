use super::super::{ColumnKind::*, ColumnSpec as Col, IndexSpec, SchemaVersion, TableSpec};

pub(super) const TABLES: &[TableSpec] = &[
    TableSpec {
        version: SchemaVersion::Initial,
        name: "credential_quota_cycle_usage",
        columns: &[
            Col::required("usage_id", Integer),
            Col::required("window_key", Text),
            Col::required("cycle_id", Integer),
        ],
        indexes: &[
            IndexSpec {
                name: "uq_cycle_usage_window",
                columns: &["usage_id", "window_key"],
                unique: true,
                added_in: None,
            },
            IndexSpec {
                name: "ix_cycle_usage_cycle",
                columns: &["cycle_id", "usage_id"],
                unique: false,
                added_in: None,
            },
        ],
    },
    TableSpec {
        version: SchemaVersion::Initial,
        name: "credential_quota_activity",
        columns: &[
            Col::required("request_id", Text),
            Col::required("credential_id", Integer),
            Col::required("model", Text),
            Col::required("started_at_ms", Integer),
        ],
        indexes: &[
            IndexSpec {
                name: "uq_quota_activity",
                columns: &["request_id", "credential_id", "started_at_ms"],
                unique: true,
                added_in: None,
            },
            IndexSpec {
                name: "ix_quota_activity_credential",
                columns: &["credential_id", "started_at_ms"],
                unique: false,
                added_in: None,
            },
        ],
    },
];

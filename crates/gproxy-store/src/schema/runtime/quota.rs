use super::super::{ColumnKind::*, ColumnSpec as Col, IndexSpec, SchemaVersion, TableSpec};

pub(super) const TABLES: &[TableSpec] = &[
    TableSpec {
        version: SchemaVersion::Runtime,
        name: "quota_windows",
        columns: &[
            Col::id(),
            Col::required("quota_id", Integer),
            Col::required("window_kind", Text),
            Col::required("window_start", Integer),
            Col::optional("reset_at", Integer),
            Col::required("cost_used", Text),
            Col::optional("active_slot", Integer),
        ],
        indexes: &[
            IndexSpec {
                name: "uq_quota_windows_period",
                columns: &["quota_id", "window_kind", "window_start"],
                unique: true,
                added_in: None,
            },
            IndexSpec {
                name: "uq_quota_windows_active",
                columns: &["quota_id", "window_kind", "active_slot"],
                unique: true,
                added_in: None,
            },
        ],
    },
    TableSpec {
        version: SchemaVersion::Runtime,
        name: "credential_quota_cycles",
        columns: &[
            Col::id(),
            Col::required("version", Integer),
            Col::required("credential_id", Integer),
            Col::required("window_key", Text),
            Col::optional("period_start", Integer),
            Col::optional("period_end", Integer),
            Col::required("boundary_source", Text),
            Col::required("boundary_confidence", Text),
            Col::required("status", Text),
            Col::optional("close_reason", Text),
            Col::optional("open_slot", Integer),
            Col::required("last_observed_at", Integer),
            Col::optional("upstream_used", Text),
            Col::optional("upstream_limit", Text),
            Col::optional("used_percent", Text),
            Col::required("coverage", Text),
            Col::required("metrics_json", Text),
        ],
        indexes: &[
            IndexSpec {
                name: "uq_credential_quota_cycles_open",
                columns: &["credential_id", "window_key", "open_slot"],
                unique: true,
                added_in: None,
            },
            IndexSpec {
                name: "ix_credential_quota_cycles_history",
                columns: &["credential_id", "window_key", "period_start", "id"],
                unique: false,
                added_in: None,
            },
        ],
    },
    TableSpec {
        version: SchemaVersion::Runtime,
        name: "quota_settlements",
        columns: &[
            Col::id(),
            Col::required("request_id", Text),
            Col::required("window_id", Integer),
            Col::required("cost", Text),
        ],
        indexes: &[IndexSpec {
            name: "uq_quota_settlements_request_window",
            columns: &["request_id", "window_id"],
            unique: true,
            added_in: None,
        }],
    },
];

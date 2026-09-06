use super::super::{
    ColumnKind::*, ColumnSpec as Col, IndexSpec, Ownership, SchemaVersion, TableSpec,
};

pub(super) const TABLES: &[TableSpec] = &[
    TableSpec {
        version: SchemaVersion::QuotaObservations,
        name: "credential_quota_observations",
        columns: &[
            Col::id(),
            Col::required("cycle_id", Integer),
            Col::required("started_at_ms", Integer),
            Col::required("observed_at_ms", Integer),
            Col::required("snapshot_json", Text),
        ],
        owns: &[],
        indexes: &[IndexSpec {
            name: "uq_credential_quota_observation",
            columns: &["cycle_id", "observed_at_ms", "started_at_ms"],
            unique: true,
            added_in: None,
        }],
    },
    TableSpec {
        version: SchemaVersion::Initial,
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
        owns: &[Ownership::Owns {
            table: "quota_settlements",
            column: "window_id",
        }],
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
        version: SchemaVersion::Initial,
        name: "credential_quota_cycles",
        columns: &[
            Col::id(),
            Col::required("accounting_start_ms", Integer),
            Col::optional("accounting_end_ms", Integer),
            Col::required("tracking_json", Text),
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
            Col::optional("label", Text),
        ],
        owns: &[
            Ownership::Owns {
                table: "credential_quota_observations",
                column: "cycle_id",
            },
            Ownership::Owns {
                table: "credential_quota_cycle_usage",
                column: "cycle_id",
            },
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
        version: SchemaVersion::Initial,
        name: "quota_settlements",
        columns: &[
            Col::id(),
            Col::required("request_id", Text),
            Col::required("window_id", Integer),
            Col::required("cost", Text),
        ],
        owns: &[],
        indexes: &[IndexSpec {
            name: "uq_quota_settlements_request_window",
            columns: &["request_id", "window_id"],
            unique: true,
            added_in: None,
        }],
    },
];

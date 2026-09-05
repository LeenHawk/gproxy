mod activity;
mod links;
mod read;
mod write;

pub(crate) use activity::*;
pub(crate) use links::*;
pub(crate) use read::*;
pub(crate) use write::*;

const COLUMNS: &[&str] = &[
    "id",
    "version",
    "credential_id",
    "window_key",
    "period_start",
    "period_end",
    "boundary_source",
    "boundary_confidence",
    "status",
    "close_reason",
    "open_slot",
    "last_observed_at",
    "upstream_used",
    "upstream_limit",
    "used_percent",
    "coverage",
    "metrics_json",
    "label",
    "accounting_start_ms",
    "accounting_end_ms",
    "tracking_json",
];

use super::{ColumnKind::*, ColumnSpec as Col, IndexSpec, SchemaVersion, TableSpec};

/// Per-model capability facts, keyed by `(provider_id, model_id)` rather than
/// a row id; `delete_provider_model` removes them by that pair, and the
/// provider owns them by `provider_id` for the wider cascade.
pub const TABLES: &[TableSpec] = &[
    TableSpec {
        version: SchemaVersion::ModelMetadata,
        name: "provider_model_modalities",
        columns: &[
            Col::id(),
            Col::required("provider_id", Integer),
            Col::required("model_id", Text),
            Col::required("direction", Text),
            Col::required("modality", Text),
            Col::required("sort_order", Integer),
        ],
        owns: &[],
        indexes: &[IndexSpec {
            name: "uq_provider_model_modalities",
            columns: &["provider_id", "model_id", "direction", "modality"],
            unique: true,
            added_in: None,
        }],
    },
    TableSpec {
        version: SchemaVersion::ModelMetadata,
        name: "provider_model_parameters",
        columns: &[
            Col::id(),
            Col::required("provider_id", Integer),
            Col::required("model_id", Text),
            Col::required("parameter", Text),
            Col::required("sort_order", Integer),
        ],
        owns: &[],
        indexes: &[IndexSpec {
            name: "uq_provider_model_parameters",
            columns: &["provider_id", "model_id", "parameter"],
            unique: true,
            added_in: None,
        }],
    },
    TableSpec {
        version: SchemaVersion::ModelMetadata,
        name: "provider_model_reasoning_levels",
        columns: &[
            Col::id(),
            Col::required("provider_id", Integer),
            Col::required("model_id", Text),
            Col::required("effort", Text),
            Col::required("description", Text),
            Col::required("sort_order", Integer),
        ],
        owns: &[],
        indexes: &[IndexSpec {
            name: "uq_provider_model_reasoning_levels",
            columns: &["provider_id", "model_id", "effort"],
            unique: true,
            added_in: None,
        }],
    },
    TableSpec {
        version: SchemaVersion::ModelMetadata,
        name: "provider_model_service_tiers",
        columns: &[
            Col::id(),
            Col::required("provider_id", Integer),
            Col::required("model_id", Text),
            Col::required("tier_id", Text),
            Col::required("name", Text),
            Col::required("description", Text),
            Col::required("sort_order", Integer),
        ],
        owns: &[],
        indexes: &[IndexSpec {
            name: "uq_provider_model_service_tiers",
            columns: &["provider_id", "model_id", "tier_id"],
            unique: true,
            added_in: None,
        }],
    },
    TableSpec {
        version: SchemaVersion::ModelMetadata,
        name: "provider_model_methods",
        columns: &[
            Col::id(),
            Col::required("provider_id", Integer),
            Col::required("model_id", Text),
            Col::required("kind", Text),
            Col::required("method", Text),
            Col::required("sort_order", Integer),
        ],
        owns: &[],
        indexes: &[IndexSpec {
            name: "uq_provider_model_methods",
            columns: &["provider_id", "model_id", "kind", "method"],
            unique: true,
            added_in: None,
        }],
    },
];

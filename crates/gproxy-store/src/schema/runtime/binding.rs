use super::super::{ColumnKind::*, ColumnSpec as Col, IndexSpec, SchemaVersion, TableSpec};

pub(super) const TABLES: &[TableSpec] = &[TableSpec {
    version: SchemaVersion::Initial,
    name: "surface_bindings",
    columns: &[
        Col::id(),
        Col::required("provider_id", Integer),
        Col::required("owner_user_id", Integer),
        Col::required("kind", Text),
        Col::required("resource_id", Text),
        Col::required("credential_id", Integer),
        Col::required("summary_json", Text),
        Col::required("created_at", Integer),
        Col::required("updated_at", Integer),
    ],
    indexes: &[
        IndexSpec {
            name: "uq_surface_bindings_resource",
            columns: &["provider_id", "owner_user_id", "kind", "resource_id"],
            unique: true,
            added_in: None,
        },
        IndexSpec {
            name: "ix_surface_bindings_list",
            columns: &["provider_id", "owner_user_id", "kind", "created_at", "id"],
            unique: false,
            added_in: None,
        },
    ],
}];

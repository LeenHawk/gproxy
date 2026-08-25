use super::{ColumnKind::*, ColumnSpec as Col, SchemaVersion, TableSpec};

pub const TABLES: &[TableSpec] = &[TableSpec {
    version: SchemaVersion::Tokenizers,
    name: "tokenizer_vocabs",
    columns: &[
        Col::required("name", Text).primary(),
        Col::required("bytes", Blob),
        Col::required("updated_at", Integer),
    ],
    indexes: &[],
}];

use super::{ColumnKind::*, ColumnSpec as Col, SchemaVersion, TableSpec};

pub const TABLES: &[TableSpec] = &[
    TableSpec {
        version: SchemaVersion::Initial,
        name: "tokenizer_vocabs",
        columns: &[
            Col::required("name", Text).primary(),
            Col::optional("repository", Text),
            Col::required("bytes", Blob),
            Col::required("updated_at", Integer),
        ],
        indexes: &[],
    },
    TableSpec {
        version: SchemaVersion::Initial,
        name: "tokenizer_auth",
        columns: &[
            Col::required("kind", Text).primary(),
            Col::required("ciphertext", Blob),
            Col::required("wrapped_key", Blob),
            Col::required("payload_nonce", Blob),
            Col::required("key_nonce", Blob),
            Col::required("updated_at", Integer),
        ],
        indexes: &[],
    },
];

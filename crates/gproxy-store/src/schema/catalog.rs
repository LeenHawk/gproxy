use super::{admin, control, identity, model_metadata, runtime, tokenizer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i64)]
pub enum SchemaVersion {
    Initial = 1,
    QuotaObservations = 2,
    ModelMetadata = 3,
    OAuthSessions = 4,
    CredentialBudgets = 5,
    /// Data-only step: routes deleted before 3.0.2 left their members and
    /// public names behind, and an orphaned name blocked every later mapping.
    RouteOwnership = 6,
    /// Data-only step: sweep every row whose declared owner is gone, once
    /// ownership became a schema fact rather than per-method code.
    OwnedRows = 7,
}

impl SchemaVersion {
    pub const ALL: [Self; 7] = [
        Self::Initial,
        Self::QuotaObservations,
        Self::ModelMetadata,
        Self::OAuthSessions,
        Self::CredentialBudgets,
        Self::RouteOwnership,
        Self::OwnedRows,
    ];
    pub const LATEST: Self = Self::OwnedRows;

    pub const fn number(self) -> i64 {
        self as i64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnKind {
    Integer,
    Text,
    Blob,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnSpec {
    pub name: &'static str,
    pub kind: ColumnKind,
    pub nullable: bool,
    pub primary_key: bool,
    pub auto_increment: bool,
    pub unique: bool,
    pub default: Option<&'static str>,
    pub added_in: Option<SchemaVersion>,
    pub nullable_in: Option<SchemaVersion>,
}

impl ColumnSpec {
    pub const fn id() -> Self {
        Self {
            name: "id",
            kind: ColumnKind::Integer,
            nullable: false,
            primary_key: true,
            auto_increment: true,
            unique: false,
            default: None,
            added_in: None,
            nullable_in: None,
        }
    }

    pub const fn required(name: &'static str, kind: ColumnKind) -> Self {
        Self {
            name,
            kind,
            nullable: false,
            primary_key: false,
            auto_increment: false,
            unique: false,
            default: None,
            added_in: None,
            nullable_in: None,
        }
    }

    pub const fn optional(name: &'static str, kind: ColumnKind) -> Self {
        Self {
            nullable: true,
            ..Self::required(name, kind)
        }
    }

    pub const fn unique(mut self) -> Self {
        self.unique = true;
        self
    }

    pub const fn primary(mut self) -> Self {
        self.primary_key = true;
        self
    }

    pub const fn default(mut self, value: &'static str) -> Self {
        self.default = Some(value);
        self
    }

    pub const fn since(mut self, version: SchemaVersion) -> Self {
        self.added_in = Some(version);
        self
    }

    pub const fn nullable_since(mut self, version: SchemaVersion) -> Self {
        self.nullable = true;
        self.nullable_in = Some(version);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexSpec {
    pub name: &'static str,
    pub columns: &'static [&'static str],
    pub unique: bool,
    pub added_in: Option<SchemaVersion>,
}

impl IndexSpec {
    pub const fn since(mut self, version: SchemaVersion) -> Self {
        self.added_in = Some(version);
        self
    }
}

/// What happens to a table that refers to this one when a row here is
/// deleted. The schema carries no database foreign keys, because the four
/// backends do not agree on them, so ownership is declared once here and
/// every delete, orphan sweep and retention prune is generated from it.
/// History tables (usage, rollups, logs, audit, quota cycles) deliberately
/// keep their references and are not owned by anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ownership {
    /// Rows of `table` whose `column` names the deleted row go with it.
    Owns {
        table: &'static str,
        column: &'static str,
    },
    /// Rows of `table` survive with `column` set to NULL.
    Detaches {
        table: &'static str,
        column: &'static str,
    },
    /// Rows of a polymorphic `table` keyed by `(subject_kind = kind, subject_id)`.
    Scoped {
        table: &'static str,
        kind: &'static str,
    },
}

impl Ownership {
    pub const fn table(self) -> &'static str {
        match self {
            Self::Owns { table, .. }
            | Self::Detaches { table, .. }
            | Self::Scoped { table, .. } => table,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableSpec {
    pub version: SchemaVersion,
    pub name: &'static str,
    pub columns: &'static [ColumnSpec],
    pub owns: &'static [Ownership],
    pub indexes: &'static [IndexSpec],
}

pub fn tables() -> impl Iterator<Item = &'static TableSpec> {
    control::TABLES
        .iter()
        .chain(model_metadata::TABLES)
        .chain(identity::TABLES)
        .chain(runtime::tables())
        .chain(tokenizer::TABLES)
        .chain(admin::TABLES)
        .chain(super::oauth::TABLES)
}

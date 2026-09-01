use super::{admin, control, identity, runtime, tokenizer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i64)]
pub enum SchemaVersion {
    Control = 1,
    Runtime = 2,
    Tokenizers = 3,
    Admin = 4,
    Pricing = 5,
    Logging = 6,
    Routing = 7,
    Process = 8,
    Configuration = 9,
    Wave26 = 10,
    Wave27 = 11,
    Wave28 = 12,
    Wave29 = 13,
    Wave30 = 14,
    Wave32 = 15,
}

impl SchemaVersion {
    pub const ALL: [Self; 15] = [
        Self::Control,
        Self::Runtime,
        Self::Tokenizers,
        Self::Admin,
        Self::Pricing,
        Self::Logging,
        Self::Routing,
        Self::Process,
        Self::Configuration,
        Self::Wave26,
        Self::Wave27,
        Self::Wave28,
        Self::Wave29,
        Self::Wave30,
        Self::Wave32,
    ];
    pub const LATEST: Self = Self::Wave32;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableSpec {
    pub version: SchemaVersion,
    pub name: &'static str,
    pub columns: &'static [ColumnSpec],
    pub indexes: &'static [IndexSpec],
}

pub fn tables() -> impl Iterator<Item = &'static TableSpec> {
    control::TABLES
        .iter()
        .chain(identity::TABLES)
        .chain(runtime::tables())
        .chain(tokenizer::TABLES)
        .chain(admin::TABLES)
        .chain(super::oauth::TABLES)
}

pub(super) fn migration_tables() -> impl Iterator<Item = &'static TableSpec> {
    tables().chain(admin::LEGACY_TABLES)
}

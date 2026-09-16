use std::collections::BTreeMap;

use sea_orm::{ColumnTrait, ColumnType, DbErr, EntityTrait, IdenStatic, Iterable};

use crate::error;

/// The Rust/SeaORM value type expected from a D1 result column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum D1Type {
    Bool,
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
    Text,
    Bytes,
    Json,
}

impl D1Type {
    pub(crate) fn from_column_type(column: &ColumnType) -> Result<Self, DbErr> {
        Ok(match column {
            ColumnType::Boolean => Self::Bool,
            ColumnType::TinyInteger => Self::I8,
            ColumnType::SmallInteger => Self::I16,
            ColumnType::Integer => Self::I32,
            ColumnType::BigInteger => Self::I64,
            ColumnType::TinyUnsigned => Self::U8,
            ColumnType::SmallUnsigned => Self::U16,
            ColumnType::Unsigned => Self::U32,
            ColumnType::BigUnsigned => Self::U64,
            ColumnType::Float => Self::F32,
            ColumnType::Double => Self::F64,
            ColumnType::String(_) | ColumnType::Text | ColumnType::Enum { .. } => Self::Text,
            ColumnType::Blob | ColumnType::Binary(_) | ColumnType::VarBinary(_) => Self::Bytes,
            ColumnType::Json | ColumnType::JsonBinary => Self::Json,
            _ => {
                return Err(error(format!(
                    "unsupported D1 entity column type: {column:?}"
                )));
            }
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Column {
    pub kind: D1Type,
    pub nullable: bool,
}

/// Explicit types for returned column names, independent of database/table names.
///
/// Use a separate projection for each entity or joined/aggregate query. An empty
/// projection is suitable for write-only connections. Unknown result aliases,
/// duplicate aliases, invalid types and unexpected NULLs fail explicitly.
#[derive(Clone, Debug, Default)]
pub struct Projection {
    pub(crate) columns: BTreeMap<String, Column>,
    pub(crate) positional: bool,
}

impl Projection {
    /// Combine compatible named projections, for example migration metadata and
    /// application queries. A conflicting alias is rejected before execution.
    pub fn merge(mut self, other: Self) -> Result<Self, DbErr> {
        if self.positional || other.positional {
            return Err(error("cannot merge positional projections"));
        }
        for (name, column) in other.columns {
            if let Some(existing) = self.columns.get(&name)
                && (existing.kind != column.kind || existing.nullable != column.nullable)
            {
                return Err(error(format!("conflicting projection alias: {name}")));
            }
            self.columns.insert(name, column);
        }
        Ok(self)
    }
    pub fn new() -> Self {
        Self::default()
    }

    /// Inspect the expected value type and nullability of an alias.
    pub fn column_type(&self, name: &str) -> Option<(D1Type, bool)> {
        self.columns
            .get(name)
            .map(|column| (column.kind, column.nullable))
    }

    /// Derive supported column types and nullability from SeaORM entity metadata.
    /// Temporal/decimal/UUID/custom SQL types require deliberate application
    /// modelling or explicit projections and are not guessed as strings.
    pub fn for_entity<E: EntityTrait>() -> Result<Self, DbErr> {
        Self::for_entity_prefixed::<E>("", false)
    }

    /// Derive columns under a SQL alias prefix. `nullable` marks the whole entity
    /// optional, as on the right of a LEFT JOIN. Normal relation queries can use
    /// SelectProjection::projection without specifying prefixes manually.
    pub fn for_entity_prefixed<E: EntityTrait>(
        prefix: &str,
        nullable: bool,
    ) -> Result<Self, DbErr> {
        let mut projection = Self::new();
        for column in E::Column::iter() {
            let definition = column.def();
            projection = projection.column(
                format!("{prefix}{}", column.as_str()),
                D1Type::from_column_type(definition.get_column_type())?,
                nullable || definition.is_null(),
            )?;
        }
        Ok(projection)
    }

    /// Add a named result column. Names must match the SQL aliases exactly.
    pub fn column(
        mut self,
        name: impl Into<String>,
        kind: D1Type,
        nullable: bool,
    ) -> Result<Self, DbErr> {
        let name = name.into();
        if name.is_empty() || self.columns.contains_key(&name) {
            return Err(error("empty or duplicate D1 projection alias"));
        }
        self.columns.insert(name, Column { kind, nullable });
        Ok(self)
    }

    /// Decode by SQL column position instead of column name.
    ///
    /// Required for `into_tuple()` / `try_get_by_index()`. Results in this mode
    /// cannot be decoded as named entity models. Type matching still uses the
    /// original SQL aliases, so joins should assign distinct aliases.
    pub fn by_index(mut self) -> Self {
        self.positional = true;
        self
    }
}

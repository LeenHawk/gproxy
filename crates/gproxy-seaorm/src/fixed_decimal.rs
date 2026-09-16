//! Exact nine-place fixed-point storage. Integers stay integers in the database;
//! explicit textual transport avoids the JavaScript Number precision boundary.

use rust_decimal::{Decimal, RoundingStrategy, prelude::ToPrimitive};
use sea_orm::sea_query::{ArrayType, ColumnType, Nullable, ValueType, ValueTypeErr};
use sea_orm::{ColIdx, DbErr, QueryResult, TryGetError, TryGetable, Value};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{fmt, str::FromStr};

/// Fixed-point value with nine decimal places. USD is the business convention
/// for money fields; the same representation also stores quantities/multipliers.
///
/// Entity fields use BigInteger with `select_as = "char(32)"` and
/// `save_as = "decimal(20,0)"`. These casts preserve signed i64 atoms through D1
/// and work with SQLite, PostgreSQL and MySQL SQL syntax. No floating point.
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FixedDecimal(i64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixedDecimalError {
    Invalid,
    Precision,
    Overflow,
}

impl fmt::Display for FixedDecimalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Invalid => "invalid decimal value",
            Self::Precision => "fixed decimal supports at most nine fractional digits",
            Self::Overflow => "fixed decimal exceeds the signed 64-bit storage range",
        })
    }
}
impl std::error::Error for FixedDecimalError {}

impl FixedDecimal {
    pub const SCALE: u32 = 9;
    pub const FACTOR: i64 = 1_000_000_000;
    pub const ZERO: Self = Self(0);
    pub const fn from_atoms(atoms: i64) -> Self {
        Self(atoms)
    }
    pub const fn atoms(self) -> i64 {
        self.0
    }
    pub fn decimal(self) -> Decimal {
        Decimal::from_i128_with_scale(i128::from(self.0), Self::SCALE)
    }
    pub fn exact(value: Decimal) -> Result<Self, FixedDecimalError> {
        let atoms = value
            .checked_mul(Decimal::from(Self::FACTOR))
            .ok_or(FixedDecimalError::Overflow)?;
        if !atoms.fract().is_zero() {
            return Err(FixedDecimalError::Precision);
        }
        atoms.to_i64().map(Self).ok_or(FixedDecimalError::Overflow)
    }
    /// Round the complete calculated charge once at the settlement boundary.
    pub fn rounded(value: Decimal) -> Result<Self, FixedDecimalError> {
        Self::exact(
            value.round_dp_with_strategy(Self::SCALE, RoundingStrategy::MidpointNearestEven),
        )
    }
    pub fn checked_add(self, other: Self) -> Result<Self, FixedDecimalError> {
        self.0
            .checked_add(other.0)
            .map(Self)
            .ok_or(FixedDecimalError::Overflow)
    }
    pub fn checked_sub(self, other: Self) -> Result<Self, FixedDecimalError> {
        self.0
            .checked_sub(other.0)
            .map(Self)
            .ok_or(FixedDecimalError::Overflow)
    }
}
impl fmt::Display for FixedDecimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.decimal().normalize().fmt(f)
    }
}
impl fmt::Debug for FixedDecimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
impl FromStr for FixedDecimal {
    type Err = FixedDecimalError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::exact(Decimal::from_str_exact(s).map_err(|_| FixedDecimalError::Invalid)?)
    }
}
impl TryFrom<Decimal> for FixedDecimal {
    type Error = FixedDecimalError;
    fn try_from(value: Decimal) -> Result<Self, Self::Error> {
        Self::exact(value)
    }
}
impl From<FixedDecimal> for Decimal {
    fn from(value: FixedDecimal) -> Self {
        value.decimal()
    }
}
impl Serialize for FixedDecimal {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}
impl<'de> Deserialize<'de> for FixedDecimal {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}
impl From<FixedDecimal> for Value {
    fn from(value: FixedDecimal) -> Self {
        Self::String(Some(value.0.to_string()))
    }
}
impl Nullable for FixedDecimal {
    fn null() -> Value {
        Value::String(None)
    }
}
impl ValueType for FixedDecimal {
    fn try_from(value: Value) -> Result<Self, ValueTypeErr> {
        match value {
            Value::String(Some(s)) => s.trim().parse().map(Self).map_err(|_| ValueTypeErr),
            Value::BigInt(Some(n)) => Ok(Self(n)),
            _ => Err(ValueTypeErr),
        }
    }
    fn type_name() -> String {
        "FixedDecimal".into()
    }
    fn array_type() -> ArrayType {
        ArrayType::BigInt
    }
    fn column_type() -> ColumnType {
        ColumnType::BigInteger
    }
}
impl TryGetable for FixedDecimal {
    fn try_get_by<I: ColIdx>(row: &QueryResult, index: I) -> Result<Self, TryGetError> {
        match i64::try_get_by(row, index) {
            Ok(value) => Ok(Self(value)),
            Err(error @ TryGetError::Null(_)) => Err(error),
            Err(_) => String::try_get_by(row, index)?
                .trim()
                .parse()
                .map(Self)
                .map_err(|_| DbErr::Type("invalid fixed decimal storage integer".into()).into()),
        }
    }
}

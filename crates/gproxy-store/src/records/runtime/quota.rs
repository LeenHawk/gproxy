use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuotaWindowRecord {
    pub id: i64,
    pub quota_id: i64,
    pub window_kind: QuotaWindowKind,
    pub window_start: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_at: Option<i64>,
    pub cost_used: Decimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaWindowKind {
    Total,
    Daily,
    Weekly,
    Monthly,
    #[serde(rename = "5h")]
    FiveHour,
    #[serde(rename = "7d")]
    SevenDay,
}

impl QuotaWindowKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Total => "total",
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::FiveHour => "5h",
            Self::SevenDay => "7d",
        }
    }

    pub fn from_name(value: &str) -> Option<Self> {
        match value {
            "total" => Some(Self::Total),
            "daily" => Some(Self::Daily),
            "weekly" => Some(Self::Weekly),
            "monthly" => Some(Self::Monthly),
            "5h" => Some(Self::FiveHour),
            "7d" => Some(Self::SevenDay),
            _ => None,
        }
    }
}

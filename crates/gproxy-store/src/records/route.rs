use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum RouteStrategy {
    #[default]
    RoundRobin,
    Weighted,
    Failover,
}

impl RouteStrategy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RoundRobin => "round_robin",
            Self::Weighted => "weighted",
            Self::Failover => "failover",
        }
    }
}

impl std::str::FromStr for RouteStrategy {
    type Err = crate::StoreError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "round_robin" => Ok(Self::RoundRobin),
            "weighted" => Ok(Self::Weighted),
            "failover" => Ok(Self::Failover),
            _ => Err(crate::StoreError::InvalidData {
                field: "route strategy",
                message: format!("unsupported strategy {value}"),
            }),
        }
    }
}

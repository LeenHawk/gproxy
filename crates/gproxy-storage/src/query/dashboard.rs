use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DashboardQuery {
    pub from_unix_ms: i64,
    pub to_unix_ms: i64,
    /// Bucket width in seconds for time-series aggregation.
    pub bucket_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DashboardKpi {
    pub total_requests: i64,
    pub success_count: i64,
    pub error_4xx_count: i64,
    pub error_5xx_count: i64,
    pub total_cost: f64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub avg_latency_ms: Option<f64>,
    pub max_latency_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DashboardTrafficBucket {
    /// Bucket start as unix epoch seconds.
    pub bucket: i64,
    pub request_count: i64,
    pub cost: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DashboardStatusBucket {
    pub bucket: i64,
    pub ok: i64,
    pub err_4xx: i64,
    pub err_5xx: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DashboardOverview {
    pub kpi: DashboardKpi,
    pub traffic: Vec<DashboardTrafficBucket>,
    pub status_codes: Vec<DashboardStatusBucket>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DashboardTopProviderRow {
    pub provider_id: Option<i64>,
    pub channel: Option<String>,
    pub request_count: i64,
    pub total_cost: f64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DashboardTopProviders {
    pub rows: Vec<DashboardTopProviderRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DashboardTopModelRow {
    pub model: Option<String>,
    pub request_count: i64,
    pub total_cost: f64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DashboardTopModels {
    pub rows: Vec<DashboardTopModelRow>,
}

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum LogLevelDto {
    Off,
    Error,
    Warn,
    #[default]
    Info,
    Debug,
    Trace,
}

impl LogLevelDto {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum LogFormatDto {
    #[default]
    Text,
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct RuntimeSettingsDto {
    pub proxy: Option<String>,
    pub inherit_system_proxy: bool,
    pub file_upload_max_in_flight: u64,
    pub cors_origins: Vec<String>,
    pub trusted_proxies: Vec<String>,
    pub max_attempts: u32,
    pub max_in_flight: u64,
    pub log_level: LogLevelDto,
    pub log_format: LogFormatDto,
}

impl Default for RuntimeSettingsDto {
    fn default() -> Self {
        Self {
            proxy: None,
            inherit_system_proxy: false,
            file_upload_max_in_flight: 0,
            cors_origins: Vec::new(),
            trusted_proxies: Vec::new(),
            max_attempts: 6,
            max_in_flight: 1024,
            log_level: LogLevelDto::Info,
            log_format: LogFormatDto::Text,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum RuntimeSettingFieldDto {
    Proxy,
    FileUploadMaxInFlight,
    CorsOrigins,
    TrustedProxies,
    MaxAttempts,
    MaxInFlight,
    LogLevel,
    LogFormat,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RuntimeSettingOverrideDto {
    pub field: RuntimeSettingFieldDto,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RuntimeSettingsStatusDto {
    pub native_controls: bool,
    pub effective: RuntimeSettingsDto,
    pub overrides: Vec<RuntimeSettingOverrideDto>,
    pub log_filter: String,
}

impl RuntimeSettingsStatusDto {
    pub fn configured(effective: RuntimeSettingsDto) -> Self {
        Self {
            native_controls: cfg!(not(target_arch = "wasm32")),
            log_filter: effective.log_level.as_str().into(),
            effective,
            overrides: Vec::new(),
        }
    }
}

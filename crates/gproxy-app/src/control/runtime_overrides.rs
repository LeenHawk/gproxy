use gproxy_admin::dto::{
    LogFormatDto, RuntimeSettingFieldDto as Field, RuntimeSettingOverrideDto, RuntimeSettingsDto,
    RuntimeSettingsStatusDto,
};

#[derive(Clone, Default)]
pub(crate) struct RuntimeOverrides {
    pub instance_id: u64,
    proxy: Option<String>,
    max_attempts: Option<u32>,
    max_in_flight: Option<u64>,
    file_upload_max_in_flight: Option<u64>,
    cors_origins: Option<Vec<String>>,
    trusted_proxies: Option<Vec<String>>,
    log_format: Option<LogFormatDto>,
    log_filter: Option<String>,
}

impl RuntimeOverrides {
    pub(crate) fn from_config(config: &crate::Config) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let native = config.native();
            Self {
                instance_id: native.instance_id,
                proxy: native.upstream_proxy_url.clone(),
                max_attempts: native.max_attempts,
                max_in_flight: native.max_in_flight.map(|value| value as u64),
                file_upload_max_in_flight: native
                    .file_upload_max_in_flight
                    .map(|value| value as u64),
                cors_origins: native.cors_origins.clone(),
                trusted_proxies: native
                    .trusted_proxies
                    .as_ref()
                    .map(|values| values.iter().map(ToString::to_string).collect()),
                log_format: native.log_format,
                log_filter: native.log_filter.clone(),
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = config;
            Self::default()
        }
    }

    pub(crate) fn status(&self, configured: RuntimeSettingsDto) -> RuntimeSettingsStatusDto {
        let mut status = RuntimeSettingsStatusDto::configured(configured);
        let mut overridden = |field, source: &str| {
            status.overrides.push(RuntimeSettingOverrideDto {
                field,
                source: source.into(),
            });
        };
        if let Some(value) = &self.proxy {
            status.effective.proxy = Some(value.clone());
            overridden(
                Field::Proxy,
                "--upstream-proxy-url / GPROXY_UPSTREAM_PROXY_URL",
            );
        }
        if let Some(value) = self.max_attempts {
            status.effective.max_attempts = value;
            overridden(Field::MaxAttempts, "--max-attempts / GPROXY_MAX_ATTEMPTS");
        }
        if let Some(value) = self.max_in_flight {
            status.effective.max_in_flight = value;
            overridden(Field::MaxInFlight, "--max-in-flight / GPROXY_MAX_IN_FLIGHT");
        }
        if let Some(value) = self.file_upload_max_in_flight {
            status.effective.file_upload_max_in_flight = value;
            overridden(
                Field::FileUploadMaxInFlight,
                "--file-upload-max-in-flight / GPROXY_FILE_UPLOAD_MAX_IN_FLIGHT",
            );
        }
        if let Some(value) = &self.cors_origins {
            status.effective.cors_origins = value.clone();
            overridden(Field::CorsOrigins, "--cors-origin / GPROXY_CORS_ORIGINS");
        }
        if let Some(value) = &self.trusted_proxies {
            status.effective.trusted_proxies = value.clone();
            overridden(
                Field::TrustedProxies,
                "--trusted-proxy / GPROXY_TRUSTED_PROXIES",
            );
        }
        if let Some(value) = self.log_format {
            status.effective.log_format = value;
            overridden(Field::LogFormat, "--log-format / GPROXY_LOG_FORMAT");
        }
        if let Some(value) = &self.log_filter {
            status.log_filter = value.clone();
            overridden(Field::LogLevel, "RUST_LOG");
        }
        status
    }
}

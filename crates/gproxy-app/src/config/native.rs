use super::{ConfigError, RawConfig, invalid};

impl RawConfig {
    pub(super) fn apply_env(&mut self) -> Result<(), ConfigError> {
        override_env("GPROXY_LISTEN_ADDR", "listen_addr", &mut self.listen_addr)?;
        override_env("GPROXY_DATA_DIR", "data_dir", &mut self.data_dir)?;
        override_env(
            "GPROXY_STORE_BACKEND",
            "store_backend",
            &mut self.store_backend,
        )?;
        override_env("GPROXY_LIBSQL_URL", "libsql_url", &mut self.libsql_url)?;
        override_env(
            "GPROXY_LIBSQL_AUTH_TOKEN",
            "libsql_auth_token",
            &mut self.libsql_auth_token,
        )?;
        override_env("GPROXY_SECRET_KEY", "secret_key", &mut self.secret_key)
    }
}

fn override_env(
    name: &'static str,
    field: &'static str,
    target: &mut Option<String>,
) -> Result<(), ConfigError> {
    match std::env::var(name) {
        Ok(value) => {
            *target = Some(value);
            Ok(())
        }
        Err(std::env::VarError::NotPresent) => Ok(()),
        Err(std::env::VarError::NotUnicode(_)) => {
            Err(invalid(field, "environment value is not UTF-8"))
        }
    }
}

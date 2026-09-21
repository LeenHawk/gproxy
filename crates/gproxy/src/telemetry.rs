//! The tracing subscriber, installed once before anything else can fail.
//!
//! Two formats, because logs are read by two different things. `text` is for a
//! person watching a terminal; `json` is for whatever ships lines to a log store
//! and wants fields rather than a sentence. There is no third option and no
//! configurable layout: a format nobody parses is a format that drifts.
//!
//! The filter is `RUST_LOG` syntax either way, resolved by [`crate::config`] so
//! that `GPROXY_LOG_FILTER` beats an ambient `RUST_LOG` rather than the other way
//! round.

use tracing_subscriber::{EnvFilter, fmt, prelude::*};

use crate::{
    Error, Result,
    config::{LogFormat, TelemetryOptions},
};

/// Install the subscriber. Idempotent by omission rather than by design: a
/// second call fails, which is why `run` makes exactly one.
pub fn init(options: &TelemetryOptions) -> Result<()> {
    let directives = directives(&options.filter);
    let filter = EnvFilter::try_new(&directives).map_err(|error| {
        Error::config(
            "--log-filter / GPROXY_LOG_FILTER",
            format!("`{}` is not a tracing filter: {error}", options.filter),
        )
    })?;
    let registry = tracing_subscriber::registry().with(filter);
    let installed = match options.format {
        // Logs go to stderr, so `export --out -` and the one-time credential
        // disclosure keep standard output to themselves. A command whose output
        // is piped into a file must not find log lines in it.
        LogFormat::Text => registry
            .with(fmt::layer().with_writer(std::io::stderr))
            .try_init(),
        LogFormat::Json => registry
            .with(fmt::layer().json().with_writer(std::io::stderr))
            .try_init(),
    };
    installed.map_err(|error| Error::other(format!("installing the log subscriber: {error}")))
}

/// The one target this binary silences, and why.
///
/// SeaORM's SQLite driver logs `Setting isolation level in a SQLite transaction
/// isn't supported` at `warn` for **every statement in every batch**. SQLite has
/// exactly one isolation level and it is the one the engine wants, so the notice
/// describes nothing an operator can act on — but there is one per statement,
/// and a startup that synchronizes the schema emits dozens. Left alone it buries
/// the lines that matter, including the plaintext-secrets warning.
const QUIET: &str = "sea_orm::driver::sqlx_sqlite=error";

/// Append [`QUIET`] unless the operator has already said something about
/// `sea_orm`.
///
/// Appended rather than baked into the default, because the noise is worst
/// exactly when someone raises the filter to debug a problem. Skipped when the
/// operator named the target themselves: they are debugging the driver, and this
/// must not overrule them.
fn directives(filter: &str) -> String {
    if filter.contains("sea_orm") {
        return filter.to_owned();
    }
    format!("{filter},{QUIET}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_malformed_filter_names_the_flag_rather_than_panicking() {
        let error = init(&TelemetryOptions {
            format: LogFormat::Text,
            filter: "=========".into(),
        })
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("--log-filter / GPROXY_LOG_FILTER"),
            "{error}"
        );
    }

    #[test]
    fn the_drivers_per_statement_notice_is_silenced_by_default() {
        assert_eq!(directives("info"), format!("info,{QUIET}"));
        assert_eq!(directives("debug"), format!("debug,{QUIET}"));
    }

    #[test]
    fn an_operator_debugging_the_driver_is_not_overruled() {
        for filter in ["info,sea_orm=debug", "sea_orm::driver::sqlx_sqlite=trace"] {
            assert_eq!(directives(filter), filter);
        }
    }
}

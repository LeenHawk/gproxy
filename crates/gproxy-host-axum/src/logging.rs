use std::io::IsTerminal;
use std::sync::{Mutex, OnceLock};

use gproxy_admin::dto::{LogFormatDto, RuntimeSettingsStatusDto};
use tracing_subscriber::{
    EnvFilter, Layer, Registry, layer::SubscriberExt, reload, util::SubscriberInitExt,
};

type FilteredRegistry =
    tracing_subscriber::layer::Layered<reload::Layer<EnvFilter, Registry>, Registry>;
type OutputLayer = Box<dyn Layer<FilteredRegistry> + Send + Sync>;

struct Logging {
    filter: reload::Handle<EnvFilter, Registry>,
    output: reload::Handle<OutputLayer, FilteredRegistry>,
    current: Mutex<(String, LogFormatDto)>,
}

static LOGGING: OnceLock<Logging> = OnceLock::new();

pub fn init(format: LogFormatDto, filter: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let filter = filter.unwrap_or("info").to_owned();
    let (filter_layer, filter_handle) = reload::Layer::new(EnvFilter::try_new(&filter)?);
    let (output_layer, output_handle) = reload::Layer::new(output(format));
    tracing_subscriber::registry()
        .with(filter_layer)
        .with(output_layer)
        .try_init()?;
    let _ = LOGGING.set(Logging {
        filter: filter_handle,
        output: output_handle,
        current: Mutex::new((filter, format)),
    });
    Ok(())
}

pub(crate) fn apply(settings: &RuntimeSettingsStatusDto) {
    let Some(logging) = LOGGING.get() else {
        return;
    };
    let mut current = logging
        .current
        .lock()
        .expect("logging configuration poisoned");
    let format = settings.effective.log_format;
    if current.0 == settings.log_filter && current.1 == format {
        return;
    }
    if current.0 != settings.log_filter {
        logging
            .filter
            .reload(EnvFilter::try_new(&settings.log_filter).expect("log filter is validated"))
            .expect("logging subscriber remains active");
    }
    if current.1 != format {
        logging
            .output
            .reload(output(format))
            .expect("logging subscriber remains active");
    }
    *current = (settings.log_filter.clone(), format);
}

fn output(format: LogFormatDto) -> OutputLayer {
    let ansi = !cfg!(windows)
        && std::io::stdout().is_terminal()
        && std::env::var_os("NO_COLOR").is_none()
        && std::env::var("TERM").as_deref() != Ok("dumb");
    match format {
        LogFormatDto::Text => tracing_subscriber::fmt::layer().with_ansi(ansi).boxed(),
        LogFormatDto::Json => tracing_subscriber::fmt::layer().json().boxed(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changing_filter_and_format_keeps_the_subscriber_registered() {
        let (filter, filter_handle) = reload::Layer::new(EnvFilter::new("info"));
        let (format, format_handle) = reload::Layer::new(output(LogFormatDto::Text));
        let subscriber = tracing_subscriber::registry().with(filter).with(format);
        let _guard = tracing::subscriber::set_default(subscriber);
        tracing::info!("before log reload");
        assert!(!tracing::enabled!(tracing::Level::DEBUG));
        filter_handle.reload(EnvFilter::new("debug")).unwrap();
        format_handle.reload(output(LogFormatDto::Json)).unwrap();
        assert!(tracing::enabled!(tracing::Level::DEBUG));
        tracing::debug!("after JSON log reload");
        format_handle.reload(output(LogFormatDto::Text)).unwrap();
        filter_handle.reload(EnvFilter::new("warn")).unwrap();
        assert!(!tracing::enabled!(tracing::Level::INFO));
        tracing::warn!("after text log reload");
    }
}

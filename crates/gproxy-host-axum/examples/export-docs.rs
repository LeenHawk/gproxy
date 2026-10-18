//! Export documentation data without opening a store, a listener or an account.

use std::{error::Error, fs, path::Path};

use gproxy_channel::{channel::HOST_CONFIG_KEYS, channels::compiled_in};
use serde::Serialize;
use serde_json::json;

fn write(directory: &Path, name: &str, value: &impl Serialize) -> Result<(), Box<dyn Error>> {
    let mut body = serde_json::to_string_pretty(value)?;
    body.push('\n');
    fs::write(directory.join(name), body)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let output = std::env::args_os()
        .nth(1)
        .ok_or("usage: export-docs OUTPUT_DIRECTORY")?;
    let output = Path::new(&output);
    fs::create_dir_all(output)?;
    write(
        output,
        "openapi.json",
        &gproxy_host_axum::documentation::openapi(),
    )?;
    write(
        output,
        "configuration.schema.json",
        &gproxy_app::config::schema(),
    )?;
    write(
        output,
        "configuration.defaults.json",
        &gproxy_app::config::AppConfig::default(),
    )?;

    let mut channels = compiled_in();
    channels.sort_by_key(|channel| channel.id());
    let channels: Vec<_> = channels
        .iter()
        .map(|channel| {
            let services: Vec<_> = channel
                .services()
                .map(|services| services.routes())
                .unwrap_or_default()
                .iter()
                .map(|route| {
                    json!({
                        "method": route.method.as_str(),
                        "path": route.path_template,
                        "transport": route.transport,
                        "class": route.class,
                        "idempotent": route.idempotent,
                    })
                })
                .collect();
            json!({ "descriptor": channel.descriptor(), "services": services })
        })
        .collect();
    if channels.is_empty() {
        return Err("enable channel features when exporting the channel catalogue".into());
    }
    write(
        output,
        "channels.json",
        &json!({
            "version": env!("CARGO_PKG_VERSION"),
            "hostConfigKeys": HOST_CONFIG_KEYS,
            "channels": channels,
        }),
    )?;
    Ok(())
}

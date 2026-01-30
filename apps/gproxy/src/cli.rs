use clap::Parser;
use serde::{Deserialize, Serialize};

#[derive(Parser)]
#[command(name = "gproxy")]
pub(crate) struct Cli {
    #[arg(long, default_value = "")]
    pub(crate) dsn: String,
    #[arg(long, default_value = "")]
    pub(crate) data_dir: String,
    #[arg(long, default_value = "127.0.0.1")]
    pub(crate) host: String,
    #[arg(long, default_value_t = 8787)]
    pub(crate) port: u16,
    #[arg(long, default_value = "pwd")]
    pub(crate) admin_key: String,
    #[arg(long)]
    pub(crate) proxy: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct GlobalConfig {
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) admin_key: String,
    pub(crate) dsn: String,
    #[serde(default)]
    pub(crate) proxy: Option<String>,
    #[serde(default)]
    pub(crate) data_dir: String,
}

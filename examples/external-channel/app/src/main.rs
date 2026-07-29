use gproxy_example_channel as _;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    gproxy::native::run_cli().await
}

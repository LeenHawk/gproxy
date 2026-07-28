//! GPROXY v2 binary: the standard native CLI over the reusable library entrypoint.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    gproxy::native::run_cli().await
}

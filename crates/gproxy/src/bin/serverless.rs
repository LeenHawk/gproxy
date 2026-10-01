use clap::Parser;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let result = async {
        let settings = gproxy::config::settings(&gproxy::Cli::parse())?;
        gproxy::telemetry::init(&settings.telemetry)?;
        gproxy::serverless::run(settings).await
    }
    .await;
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("gproxy-serverless: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

//! The `gproxy` binary.
//!
//! Three things happen here and nowhere else, in this order, because each one
//! depends on the last:
//!
//! 1. `.env` is merged into the process environment. This has to be first,
//!    because it is what `clap` then reads — and it has to be before the runtime
//!    exists, because writing to the environment is only sound while the process
//!    is single-threaded.
//! 2. The arguments are parsed. `clap` exits on `--help`, `--version` and a
//!    usage error, which is what it should do.
//! 3. The runtime is built and [`gproxy::run`] does the work.
//!
//! A failure is printed once, to standard error, and becomes exit code 1. It is
//! not `Result`-returned from `main`, because that would print the `Debug` form
//! of the error next to the word `Error:` — and every message in
//! [`gproxy::Error`] is already written for the person reading it.

use clap::Parser;

fn main() -> std::process::ExitCode {
    // Before the runtime, before `clap`: see the module note.
    if let Err(error) = gproxy::env::load_default() {
        eprintln!("gproxy: {error}");
        return std::process::ExitCode::FAILURE;
    }
    let cli = gproxy::Cli::parse();

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("gproxy: cannot start the async runtime: {error}");
            return std::process::ExitCode::FAILURE;
        }
    };
    match runtime.block_on(gproxy::run(cli)) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            // The log subscriber may or may not be installed by the time a
            // failure happens — a bad `--log-filter` is itself one of the
            // failures — so this goes straight to standard error.
            eprintln!("gproxy: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

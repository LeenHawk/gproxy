//! The `gproxy` binary: [`gproxy::main_with`] with no channels of its own.
//!
//! Three things happen there and nowhere else, in this order, because each one
//! depends on the last:
//!
//! 1. `.env` is merged into the process environment. This has to be first,
//!    because it is what `clap` then reads — and it has to be before the runtime
//!    exists, because writing to the environment is only sound while the process
//!    is single-threaded.
//! 2. The arguments are parsed. `clap` exits on `--help`, `--version` and a
//!    usage error, which is what it should do.
//! 3. The runtime is built and [`gproxy::run_with`] does the work.
//!
//! A failure is printed once, to standard error, and becomes exit code 1. It is
//! not `Result`-returned from `main`, because that would print the `Debug` form
//! of the error next to the word `Error:` — and every message in
//! [`gproxy::Error`] is already written for the person reading it.

fn main() -> std::process::ExitCode {
    gproxy::main_with(Vec::new())
}

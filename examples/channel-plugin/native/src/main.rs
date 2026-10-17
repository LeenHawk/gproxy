//! The stock `gproxy` binary plus [`Acme`]: same commands, same flags, same
//! console, and one more channel in the provider form.

use std::{process::ExitCode, sync::Arc};

use gproxy_example_channel::Acme;

fn main() -> ExitCode {
    gproxy::main_with(vec![Arc::new(Acme)])
}

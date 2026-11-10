//! The `terminalert` binary. Everything it does is [`terminalert::cli`].

use std::process::ExitCode;

use clap::Parser;

fn main() -> ExitCode {
    ExitCode::from(terminalert::cli::Cli::parse().run())
}

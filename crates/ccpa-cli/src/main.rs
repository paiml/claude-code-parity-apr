//! `ccpa` CLI entry point.

use std::process::ExitCode;

fn main() -> ExitCode {
    ccpa_cli::run(std::env::args_os())
}

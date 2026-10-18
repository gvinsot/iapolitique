//! `iapolitique` command line: see `iapolitique --help` and AGENTS.md §13.1.

use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, Subcommand};

use iapolitique::healthcheck;

#[derive(Debug, Parser)]
#[command(name = "iapolitique", version, about = "iapolitique backend")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// HTTP GET on a local health URL; exit code 0 when it answers 2xx (Docker healthcheck).
    Healthcheck {
        /// URL to check (plain http).
        #[arg(long, default_value = "http://127.0.0.1:8080/api/health")]
        url: String,
        /// Connect and read timeout, in seconds.
        #[arg(long, default_value_t = 3)]
        timeout_secs: u64,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Healthcheck { url, timeout_secs } => {
            match healthcheck::check(&url, Duration::from_secs(timeout_secs)) {
                Ok(_) => ExitCode::SUCCESS,
                Err(err) => {
                    eprintln!("healthcheck failed: {err}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_is_well_formed() {
        Cli::command().debug_assert();
    }
}

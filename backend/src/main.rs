//! `iapolitique` command line: see `iapolitique --help` and AGENTS.md §13.1.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, Subcommand};

use iapolitique::config::{self, Config, Role};
use iapolitique::http::openapi;
use iapolitique::{app, db, healthcheck, jobs, telemetry};

#[derive(Debug, Parser)]
#[command(name = "iapolitique", version, about = "iapolitique backend")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Public API on PUBLIC_PORT (8080) and admin API on ADMIN_PORT (8081); applies migrations
    /// at startup.
    Api,
    /// Background worker: consumes the job queue; internal health on WORKER_HEALTH_PORT (8082).
    Worker,
    /// Apply pending database migrations, then exit.
    Migrate,
    /// Print the OpenAPI specification (public and admin routes) as JSON.
    Openapi {
        /// Write to this file instead of stdout (UTF-8, LF line endings).
        #[arg(long, short)]
        output: Option<PathBuf>,
        /// Only the public routes, as served on the public port.
        #[arg(long)]
        public: bool,
    },
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

impl Command {
    /// Role of the command when it needs the configuration, `None` for offline commands.
    fn role(&self) -> Option<Role> {
        match self {
            Self::Api => Some(Role::Api),
            Self::Worker => Some(Role::Worker),
            Self::Migrate => Some(Role::Migrate),
            Self::Openapi { .. } | Self::Healthcheck { .. } => None,
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let Some(role) = cli.command.role() else {
        return run_offline(cli.command);
    };

    // SAFETY: still single-threaded, the async runtime is not started yet.
    unsafe { config::load_secrets_into_env(Path::new(config::DEFAULT_SECRETS_DIR)) };
    let config = match Config::from_env(role) {
        Ok(config) => config,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::from(2);
        }
    };
    let job_logs = match telemetry::init(&config.log, matches!(cli.command, Command::Worker)) {
        Ok(receiver) => receiver,
        Err(err) => {
            eprintln!("{err:#}");
            return ExitCode::from(2);
        }
    };
    telemetry::install_panic_hook();

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => {
            tracing::error!(error = %err, "cannot start the async runtime");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(run(cli.command, config, job_logs)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            tracing::error!(error = format!("{err:#}"), "fatal error");
            ExitCode::FAILURE
        }
    }
}

/// Commands that need neither configuration nor database.
fn run_offline(command: Command) -> ExitCode {
    match command {
        Command::Openapi { output, public } => {
            let spec = if public {
                openapi::public_spec()
            } else {
                openapi::full_spec()
            };
            let json = openapi::to_json(&spec);
            match output {
                None => {
                    print!("{json}");
                    ExitCode::SUCCESS
                }
                Some(path) => match std::fs::write(&path, json) {
                    Ok(()) => ExitCode::SUCCESS,
                    Err(err) => {
                        eprintln!("cannot write {}: {err}", path.display());
                        ExitCode::FAILURE
                    }
                },
            }
        }
        Command::Healthcheck { url, timeout_secs } => {
            match healthcheck::check(&url, Duration::from_secs(timeout_secs)) {
                Ok(_) => ExitCode::SUCCESS,
                Err(err) => {
                    eprintln!("healthcheck failed: {err}");
                    ExitCode::FAILURE
                }
            }
        }
        Command::Api | Command::Worker | Command::Migrate => {
            unreachable!("needs the configuration")
        }
    }
}

async fn run(
    command: Command,
    config: Config,
    job_logs: Option<jobs::logs::JobLogReceiver>,
) -> anyhow::Result<()> {
    match command {
        Command::Api => app::run_api(config).await,
        Command::Worker => app::run_worker(config, job_logs).await,
        Command::Migrate => {
            let pool = db::connect(&config.database, "iapolitique-migrate", Duration::ZERO).await?;
            db::migrate(&pool).await?;
            tracing::info!("migrations applied");
            pool.close().await;
            Ok(())
        }
        Command::Openapi { .. } | Command::Healthcheck { .. } => unreachable!("handled offline"),
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

    #[test]
    fn roles_of_commands() {
        let role = |args: &[&str]| Cli::try_parse_from(args).unwrap().command.role();
        assert_eq!(role(&["iapolitique", "api"]), Some(Role::Api));
        assert_eq!(role(&["iapolitique", "worker"]), Some(Role::Worker));
        assert_eq!(role(&["iapolitique", "migrate"]), Some(Role::Migrate));
        assert_eq!(role(&["iapolitique", "openapi", "--public"]), None);
        assert_eq!(
            role(&["iapolitique", "healthcheck", "--url", "http://x/"]),
            None
        );
    }
}

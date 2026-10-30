#[path = "coding-data/backfill.rs"]
mod backfill;
#[path = "coding-data/repair.rs"]
mod repair;
#[path = "coding-data/watch.rs"]
mod watch;

use anyhow::Result;
use clap::Parser;
use coding_data::config::{Cli, Command, Config};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let config_path = cli.config.clone().or_else(|| {
        let default = Config::default_config_path();
        default.exists().then_some(default)
    });
    let mut config = Config::load(config_path.as_deref(), cli.source)?;
    config.apply_cli_overrides(&cli)?;

    match cli.command {
        Command::Backfill { project, dry_run } => {
            backfill::run(config, project, dry_run).await?;
        }
        Command::Watch { sources } => {
            watch::run(config, sources).await?;
        }
        Command::Repair { session, dry_run } => {
            repair::run(config, session, dry_run).await?;
        }
    }

    Ok(())
}

use crate::batch::DEFAULT_BATCH_SIZE;
use crate::source::Source;
use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub source: Source,
    pub source_dir: PathBuf,
    pub data_dir: PathBuf,
    pub sqlite_path: Option<PathBuf>,
    #[serde(default = "default_source_host")]
    pub source_host: String,
    pub graphql_endpoint: String,
    pub batch_size: usize,
    pub backfill_workers: usize,
    pub scan_interval_secs: u64,
    pub reconcile_interval_secs: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct FileConfig {
    source_dir: Option<PathBuf>,
    data_dir: Option<PathBuf>,
    sqlite_path: Option<PathBuf>,
    source_host: Option<String>,
    graphql_endpoint: Option<String>,
    batch_size: Option<usize>,
    backfill_workers: Option<usize>,
    scan_interval_secs: Option<u64>,
    reconcile_interval_secs: Option<u64>,
}

impl Default for Config {
    fn default() -> Self {
        Self::for_source(Source::Claude)
    }
}

impl Config {
    pub fn for_source(source: Source) -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        Self {
            source,
            source_dir: source.default_source_dir(),
            data_dir: home.join(".coding-data").join("data"),
            sqlite_path: source.default_sqlite_path(),
            source_host: default_source_host(),
            graphql_endpoint: String::new(),
            batch_size: DEFAULT_BATCH_SIZE,
            backfill_workers: 4,
            scan_interval_secs: 5,
            reconcile_interval_secs: source.default_reconcile_interval_secs(),
        }
    }

    pub fn load(path: Option<&Path>, cli_source: Source) -> Result<Self> {
        let file_config = match path {
            Some(path) if path.exists() => {
                let contents = std::fs::read_to_string(path)
                    .with_context(|| format!("reading config file {}", path.display()))?;
                toml::from_str::<FileConfig>(&contents)
                    .with_context(|| format!("parsing config file {}", path.display()))?
            }
            _ => FileConfig::default(),
        };

        let source = cli_source;
        let mut config = Self::for_source(source);
        config.apply_file_overrides(&file_config);
        config.apply_env_overrides();
        config.expand_paths();
        Ok(config)
    }

    pub fn apply_cli_overrides(&mut self, cli: &Cli) -> Result<()> {
        if let Some(source_dir) = &cli.source_dir {
            self.source_dir = source_dir.clone();
        }
        if let Some(data_dir) = &cli.data_dir {
            self.data_dir = data_dir.clone();
        }
        if let Some(sqlite_path) = &cli.sqlite_path {
            self.sqlite_path = Some(sqlite_path.clone());
        }
        if let Some(source_host) = normalize_source_host(cli.source_host.as_deref()) {
            self.source_host = source_host;
        }
        if let Some(graphql_endpoint) = &cli.graphql_endpoint {
            self.graphql_endpoint = graphql_endpoint.clone();
        }
        if let Some(batch_size) = cli.batch_size {
            self.batch_size = normalize_batch_size(batch_size);
        }
        if let Some(backfill_workers) = cli.backfill_workers {
            self.backfill_workers = normalize_worker_count(backfill_workers);
        }
        if let Some(scan_interval_secs) = cli.scan_interval_secs {
            self.scan_interval_secs = scan_interval_secs;
        }
        if let Some(reconcile_interval_secs) = cli.reconcile_interval_secs {
            self.reconcile_interval_secs = Some(reconcile_interval_secs);
        }

        self.expand_paths();
        self.validate()
    }

    pub fn default_config_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".coding-data")
            .join("config.toml")
    }

    pub fn state_path(&self) -> PathBuf {
        self.data_dir
            .join(self.source.as_str())
            .join("ingest-state.json")
    }

    pub fn for_runtime_source(&self, source: Source) -> Self {
        if self.source == source {
            return self.clone();
        }

        Self {
            source,
            source_dir: source.default_source_dir(),
            data_dir: self.data_dir.clone(),
            sqlite_path: source.default_sqlite_path(),
            source_host: self.source_host.clone(),
            graphql_endpoint: self.graphql_endpoint.clone(),
            batch_size: self.batch_size,
            backfill_workers: self.backfill_workers,
            scan_interval_secs: self.scan_interval_secs,
            reconcile_interval_secs: source.default_reconcile_interval_secs(),
        }
    }

    fn apply_file_overrides(&mut self, file: &FileConfig) {
        if let Some(source_dir) = &file.source_dir {
            self.source_dir = source_dir.clone();
        }
        if let Some(data_dir) = &file.data_dir {
            self.data_dir = data_dir.clone();
        }
        if let Some(sqlite_path) = &file.sqlite_path {
            self.sqlite_path = Some(sqlite_path.clone());
        }
        if let Some(source_host) = normalize_source_host(file.source_host.as_deref()) {
            self.source_host = source_host;
        }
        if let Some(graphql_endpoint) = &file.graphql_endpoint {
            self.graphql_endpoint = graphql_endpoint.clone();
        }
        if let Some(batch_size) = file.batch_size {
            self.batch_size = normalize_batch_size(batch_size);
        }
        if let Some(backfill_workers) = file.backfill_workers {
            self.backfill_workers = normalize_worker_count(backfill_workers);
        }
        if let Some(scan_interval_secs) = file.scan_interval_secs {
            self.scan_interval_secs = scan_interval_secs;
        }
        if let Some(reconcile_interval_secs) = file.reconcile_interval_secs {
            self.reconcile_interval_secs = Some(reconcile_interval_secs);
        }
    }

    fn apply_env_overrides(&mut self) {
        if let Ok(value) = std::env::var("CODING_DATA_SOURCE_DIR") {
            self.source_dir = PathBuf::from(value);
        }
        if let Ok(value) = std::env::var("CODING_DATA_DATA_DIR") {
            self.data_dir = PathBuf::from(value);
        }
        if let Ok(value) = std::env::var("CODING_DATA_SQLITE_PATH") {
            self.sqlite_path = Some(PathBuf::from(value));
        }
        if let Some(value) = std::env::var("CODING_DATA_SOURCE_HOST")
            .ok()
            .and_then(|value| normalize_source_host(Some(value.as_str())))
        {
            self.source_host = value;
        }
        if let Ok(value) = std::env::var("CODING_DATA_GRAPHQL_ENDPOINT") {
            self.graphql_endpoint = value;
        }
        if let Ok(value) = std::env::var("CODING_DATA_BATCH_SIZE") {
            if let Ok(parsed) = value.parse::<usize>() {
                self.batch_size = normalize_batch_size(parsed);
            }
        }
        if let Ok(value) = std::env::var("CODING_DATA_BACKFILL_WORKERS") {
            if let Ok(parsed) = value.parse::<usize>() {
                self.backfill_workers = normalize_worker_count(parsed);
            }
        }
        if let Ok(value) = std::env::var("CODING_DATA_SCAN_INTERVAL_SECS") {
            if let Ok(parsed) = value.parse::<u64>() {
                self.scan_interval_secs = parsed;
            }
        }
        if let Ok(value) = std::env::var("CODING_DATA_RECONCILE_INTERVAL_SECS") {
            if let Ok(parsed) = value.parse::<u64>() {
                self.reconcile_interval_secs = Some(parsed);
            }
        }
    }

    fn expand_paths(&mut self) {
        self.source_dir = expand_path(&self.source_dir);
        self.data_dir = expand_path(&self.data_dir);
        self.sqlite_path = self.sqlite_path.as_ref().map(|path| expand_path(path));
    }

    fn validate(&self) -> Result<()> {
        if self.graphql_endpoint.trim().is_empty() {
            bail!("graphql_endpoint is required");
        }
        Ok(())
    }
}

fn normalize_batch_size(batch_size: usize) -> usize {
    if batch_size == 0 {
        DEFAULT_BATCH_SIZE
    } else {
        batch_size
    }
}

fn normalize_worker_count(worker_count: usize) -> usize {
    worker_count.max(1)
}

pub fn default_source_host() -> String {
    std::env::var("HOSTNAME")
        .ok()
        .and_then(|value| normalize_source_host(Some(value.as_str())))
        .or_else(|| {
            std::env::var("COMPUTERNAME")
                .ok()
                .and_then(|value| normalize_source_host(Some(value.as_str())))
        })
        .or_else(|| {
            ProcessCommand::new("hostname")
                .output()
                .ok()
                .and_then(|output| String::from_utf8(output.stdout).ok())
                .and_then(|value| normalize_source_host(Some(value.as_str())))
        })
        .unwrap_or_else(|| "unknown".to_string())
}

fn normalize_source_host(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[derive(Debug, Parser)]
#[command(name = "coding-data", about = "Unified coding session ingest client")]
pub struct Cli {
    #[arg(long, global = true, value_enum, default_value_t = Source::Claude)]
    pub source: Source,
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,
    #[arg(long, global = true)]
    pub source_dir: Option<PathBuf>,
    #[arg(long, global = true)]
    pub data_dir: Option<PathBuf>,
    #[arg(long, global = true)]
    pub sqlite_path: Option<PathBuf>,
    #[arg(long, global = true)]
    pub source_host: Option<String>,
    #[arg(long, global = true)]
    pub graphql_endpoint: Option<String>,
    #[arg(long, global = true)]
    pub batch_size: Option<usize>,
    #[arg(long, global = true)]
    pub backfill_workers: Option<usize>,
    #[arg(long, global = true)]
    pub scan_interval_secs: Option<u64>,
    #[arg(long, global = true)]
    pub reconcile_interval_secs: Option<u64>,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Backfill {
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        dry_run: bool,
    },
    Watch {
        #[arg(long, value_enum, value_delimiter = ',')]
        sources: Vec<Source>,
    },
    /// Find sessions with missing or partial message ingestion and re-ingest them.
    Repair {
        /// Only repair a specific session by ID.
        #[arg(long)]
        session: Option<String>,
        /// Preview what would be repaired without making changes.
        #[arg(long)]
        dry_run: bool,
    },
}

fn expand_path(path: &Path) -> PathBuf {
    let raw = path.to_string_lossy();
    if raw == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    }

    if let Some(stripped) = raw.strip_prefix("~/") {
        return dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(stripped);
    }

    path.to_path_buf()
}

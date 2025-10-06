use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;
use tracing::{info, Level};
use tracing_subscriber::EnvFilter;

use skelz::{
    default_cluster_rpc_url, default_config_file_path, expand_tilde, get_config_value,
    load_config_with_overrides, save_default_config, set_config_value, sign_memo, write_config_file,
    SkelzConfig,
};

#[derive(Debug, Parser)]
#[command(name = "skelz", version, about = "Skelz CLI")] 
struct Cli {
    /// Increase verbosity (-v, -vv)
    #[arg(short = 'v', action = clap::ArgAction::Count)]
    verbose: u8,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Manage configuration
    #[command(subcommand)]
    Config(ConfigCommand),
    /// Publish a text memo transaction on Solana
    Sign(SignCmd),
    /// Verify (placeholder)
    Verify(VerifyCmd),
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    /// Generate a configuration file (TOML)
    Init(ConfigInitCmd),
    /// Get current config settings
    Get(ConfigGetCmd),
    /// Set a config setting
    Set(ConfigSetCmd),
}

#[derive(Debug, Args)]
struct ConfigInitCmd {
    /// Output path for the config file. Defaults to XDG config dir.
    #[arg(short = 'o', long = "output")]
    output: Option<PathBuf>,
    /// Overwrite existing file if present
    #[arg(long = "force")]
    force: bool,
    /// Cluster shortcut: devnet|testnet|mainnet-beta|localnet
    #[arg(long = "cluster")]
    cluster: Option<String>,
    /// RPC URL (overrides cluster default)
    #[arg(long = "rpc-url")]
    rpc_url: Option<String>,
    /// Path to Solana keypair (id.json)
    #[arg(long = "keypair")]
    keypair_path: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct ConfigGetCmd {
    /// Optional config key to read (cluster|rpc_url|keypair_path|commitment). If omitted, prints full config.
    key: Option<String>,
}

#[derive(Debug, Args)]
struct ConfigSetCmd {
    /// Config key to set (cluster|rpc_url|keypair_path|commitment)
    key: String,
    /// Value to set
    value: String,
}

#[derive(Debug, Args)]
struct SignCmd {
    /// Text message to publish on-chain via Memo program
    message: String,
    /// RPC URL (overrides config and env)
    #[arg(long = "rpc-url")]
    rpc_url: Option<String>,
    /// Path to Solana keypair (id.json) (overrides config and env)
    #[arg(long = "keypair")]
    keypair_path: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct VerifyCmd {}

fn init_tracing(verbosity: u8) {
    let level = match verbosity {
        0 => Level::INFO,
        1 => Level::DEBUG,
        _ => Level::TRACE,
    };
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level.as_str().to_string()));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    match cli.command {
        Commands::Config(cmd) => match cmd {
            ConfigCommand::Init(cmd) => {
                let mut cfg = SkelzConfig::default();
                if let Some(cluster) = cmd.cluster.as_deref() {
                    cfg.cluster = cluster.to_string();
                    cfg.rpc_url = default_cluster_rpc_url(cluster);
                }
                if let Some(rpc) = cmd.rpc_url.as_deref() {
                    cfg.rpc_url = rpc.to_string();
                }
                if let Some(path) = cmd.keypair_path.as_deref() {
                    cfg.keypair_path = expand_tilde(path);
                }

                let output_path = cmd
                    .output
                    .as_deref()
                    .map(expand_tilde)
                    .unwrap_or_else(default_config_file_path);

                write_config_file(&output_path, &cfg, cmd.force)?;
                println!(
                    "Wrote config to {}\ncluster={}\nrpc_url={}\nkeypair_path={}",
                    output_path.display(),
                    cfg.cluster,
                    cfg.rpc_url,
                    cfg.keypair_path.display()
                );
                Ok(())
            }
            ConfigCommand::Get(cmd) => {
                let cfg = skelz::read_config_file().or_else(|_| {
                    let cfg = SkelzConfig::default();
                    save_default_config(&cfg).ok();
                    Ok::<SkelzConfig, anyhow::Error>(cfg)
                })?;
                if let Some(key) = cmd.key.as_deref() {
                    let value = get_config_value(&cfg, key)?;
                    println!("{}", value);
                } else {
                    let toml_string = toml::to_string_pretty(&cfg)?;
                    println!("{}", toml_string);
                }
                Ok(())
            }
            ConfigCommand::Set(cmd) => {
                let mut cfg = skelz::read_config_file().unwrap_or_default();
                set_config_value(&mut cfg, &cmd.key, &cmd.value)?;
                save_default_config(&cfg)?;
                println!("updated {}", cmd.key);
                Ok(())
            }
        },
        Commands::Sign(cmd) => {
            let config = load_config_with_overrides(cmd.rpc_url.clone(), cmd.keypair_path.clone())?;
            let signature = sign_memo(&cmd.message, &config)?;
            info!(%signature, "memo transaction sent");
            println!("Signature={}", signature);
            Ok(())
        }
        Commands::Verify(_cmd) => {
            println!("verify: not implemented yet");
            Ok(())
        }
    }
}

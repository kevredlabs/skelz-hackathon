use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;
use tracing::{info, Level};
use tracing_subscriber::EnvFilter;

use skelz::{
    default_cluster_rpc_url, default_config_file_path, expand_tilde, get_config_value,
    load_config_with_overrides, resolve_ghcr_credentials, save_default_config, set_config_value,
    write_config_file, sign_image_with_oci, SkelzConfig,
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
    /// Sign a Docker image with Solana signature and upload to OCI registry
    Sign(SignCmd),
    /// Verify (placeholder)
    Verify(VerifyCmd),
    /// Registry operations
    #[command(subcommand)]
    Registry(RegistryCommand),
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

#[derive(Debug, Subcommand)]
enum RegistryCommand {
    /// Log into GitHub Container Registry (GHCR) using env/TOML creds
    Login(RegistryLoginCmd),
}

#[derive(Debug, Args)]
struct RegistryLoginCmd {
    /// Registry hostname (default: ghcr.io)
    #[arg(long = "registry", default_value = "ghcr.io")]
    registry: String,
    /// Username override (else resolved via env/TOML)
    #[arg(long = "username")]
    username: Option<String>,
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
    /// Canonical image reference with digest (e.g., docker.io/tonorg/tonimage@sha256:abc123...)
    image_reference: String,
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
            
            // Validate canonical reference format
            if !cmd.image_reference.contains("@sha256:") {
                return Err(anyhow::anyhow!("Image reference must be canonical with digest (e.g., ghcr.io/username/repo@sha256:abc123...)"));
            }
            
            // Validate GHCR reference
            if !cmd.image_reference.starts_with("ghcr.io") {
                return Err(anyhow::anyhow!("Only GitHub Container Registry is supported. Use format: ghcr.io/username/repo@sha256:abc123..."));
            }
            
            // Resolve GHCR authentication credentials from config
            let (username, token) = resolve_ghcr_credentials(&config)?;
            
            // Sign image and upload to OCI registry
            let signature = sign_image_with_oci(&cmd.image_reference, &config, &username, &token)?;
            
            info!(%signature, "image signed and uploaded to GHCR");
            println!("Image Signature={}", signature);
            println!("Artifact uploaded to GHCR: {}", cmd.image_reference);
            Ok(())
        }
        Commands::Verify(_cmd) => {
            println!("verify: not implemented yet");
            Ok(())
        }
        Commands::Registry(cmd) => match cmd {
            RegistryCommand::Login(cmd) => {
                let cfg = skelz::read_config_file().unwrap_or_default();
                let (mut login, pass) = resolve_ghcr_credentials(&cfg)?;
                if let Some(user_override) = cmd.username.as_deref() {
                    login = user_override.to_string();
                }

                // Non-interactive docker login: pass via stdin
                let mut child = std::process::Command::new("docker")
                    .arg("login")
                    .arg(cmd.registry)
                    .arg("-u")
                    .arg(login)
                    .arg("--password-stdin")
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::inherit())
                    .stderr(std::process::Stdio::inherit())
                    .spawn()?;

                use std::io::Write as _;
                if let Some(stdin) = child.stdin.as_mut() {
                    stdin.write_all(pass.as_bytes())?;
                }
                let status = child.wait()?;
                if !status.success() {
                    anyhow::bail!("docker login failed with status {}", status);
                }
                println!("ghcr login: success");
                Ok(())
            }
        },
    }
}

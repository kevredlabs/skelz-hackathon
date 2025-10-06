use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use solana_client::rpc_client::RpcClient;
use solana_sdk::hash::Hash;
use solana_sdk::instruction::Instruction;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{read_keypair_file, Signer};
use solana_sdk::transaction::Transaction;
use thiserror::Error;
use tracing::info;

#[derive(Debug, Error)]
pub enum SkelzError {
    #[error("config file exists: {0}")]
    ConfigExists(String),
    #[error("config not found: {0}")]
    ConfigNotFound(String),
    #[error("unknown config key: {0}")]
    UnknownConfigKey(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkelzConfig {
    pub cluster: String,
    pub rpc_url: String,
    pub keypair_path: PathBuf,
    pub commitment: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docker_login: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docker_pass: Option<String>,
}

impl Default for SkelzConfig {
    fn default() -> Self {
        Self {
            cluster: "devnet".to_string(),
            rpc_url: default_cluster_rpc_url("devnet"),
            keypair_path: default_solana_keypair_path(),
            commitment: "confirmed".to_string(),
            docker_login: None,
            docker_pass: None,
        }
    }
}

pub fn write_config_file(path: &Path, cfg: &SkelzConfig, force: bool) -> Result<()> {
    if path.exists() && !force {
        return Err(SkelzError::ConfigExists(path.display().to_string()).into());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create directory {}", parent.display()))?;
    }
    let toml_string = toml::to_string_pretty(cfg)?;
    let mut file = fs::File::create(path)
        .with_context(|| format!("create file {}", path.display()))?;
    file.write_all(toml_string.as_bytes())
        .with_context(|| format!("write file {}", path.display()))?;
    Ok(())
}

pub fn save_default_config(cfg: &SkelzConfig) -> Result<()> {
    let path = default_config_file_path();
    write_config_file(&path, cfg, true)
}

pub fn read_config_file() -> Result<SkelzConfig> {
    let path = default_config_file_path();
    if !path.exists() {
        return Err(SkelzError::ConfigNotFound(path.display().to_string()).into());
    }
    let bytes = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    let cfg: SkelzConfig = toml::from_str(std::str::from_utf8(&bytes).context("utf8 config")?)
        .with_context(|| format!("parse TOML at {}", path.display()))?;
    Ok(cfg)
}

pub fn resolve_dockerhub_credentials(cfg: &SkelzConfig) -> Result<(String, String)> {
    let env_login = std::env::var("DOCKERHUB_LOGIN").ok().filter(|v| !v.trim().is_empty());
    let env_pass = std::env::var("DOCKERHUB_PASS").ok().filter(|v| !v.trim().is_empty());

    let login = env_login.or_else(|| cfg.docker_login.clone());
    let pass = env_pass.or_else(|| cfg.docker_pass.clone());

    match (login, pass) {
        (Some(l), Some(p)) => Ok((l, p)),
        _ => Err(anyhow!(
            "DockerHub credentials not found. Set DOCKERHUB_LOGIN/DOCKERHUB_PASS or set docker_login/docker_pass in config.toml"
        )),
    }
}

pub fn load_config_with_overrides(
    rpc_url: Option<String>,
    keypair_path: Option<PathBuf>,
) -> Result<SkelzConfig> {
    let mut cfg = read_config_file().unwrap_or_default();
    if let Some(rpc) = rpc_url {
        cfg.rpc_url = rpc;
    } else if let Ok(env_rpc) = std::env::var("SOLANA_RPC_URL") {
        if !env_rpc.trim().is_empty() {
            cfg.rpc_url = env_rpc;
        }
    }
    if let Some(kp) = keypair_path.as_deref().map(expand_tilde) {
        cfg.keypair_path = kp;
    } else if let Ok(env_kp) = std::env::var("SOLANA_KEYPAIR") {
        if !env_kp.trim().is_empty() {
            cfg.keypair_path = expand_tilde(Path::new(&env_kp));
        }
    }
    Ok(cfg)
}

pub fn sign_memo(message: &str, cfg: &SkelzConfig) -> Result<String> {
    let rpc_client = RpcClient::new(cfg.rpc_url.clone());
    let payer = read_keypair_file(&cfg.keypair_path)
        .map_err(|e| anyhow!("read keypair at {}: {}", cfg.keypair_path.display(), e))?;

    let recent_blockhash: Hash = rpc_client
        .get_latest_blockhash()
        .context("fetch latest blockhash from RPC")?;

    // Correct Memo program id (v2)
    let memo_program_id = Pubkey::from_str("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr").unwrap();
    let instruction = Instruction {
        program_id: memo_program_id,
        accounts: vec![],
        data: message.as_bytes().to_vec(),
    };

    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );

    let signature = rpc_client
        .send_and_confirm_transaction(&transaction)
        .context("send and confirm transaction")?;
    info!(%signature, "memo published");
    Ok(signature.to_string())
}

pub fn get_config_value(cfg: &SkelzConfig, key: &str) -> Result<String> {
    match key {
        "cluster" => Ok(cfg.cluster.clone()),
        "rpc_url" => Ok(cfg.rpc_url.clone()),
        "keypair_path" => Ok(cfg.keypair_path.display().to_string()),
        "commitment" => Ok(cfg.commitment.clone()),
        "docker_login" => Ok(cfg.docker_login.clone().unwrap_or_default()),
        // Do not print secrets in clear text
        "docker_pass" => Ok("<redacted>".to_string()),
        _ => Err(SkelzError::UnknownConfigKey(key.to_string()).into()),
    }
}

pub fn set_config_value(cfg: &mut SkelzConfig, key: &str, value: &str) -> Result<()> {
    match key {
        "cluster" => cfg.cluster = value.to_string(),
        "rpc_url" => cfg.rpc_url = value.to_string(),
        "keypair_path" => cfg.keypair_path = expand_tilde(Path::new(value)),
        "commitment" => cfg.commitment = value.to_string(),
        "docker_login" => cfg.docker_login = Some(value.to_string()),
        "docker_pass" => cfg.docker_pass = Some(value.to_string()),
        _ => return Err(SkelzError::UnknownConfigKey(key.to_string()).into()),
    }
    Ok(())
}

pub fn default_config_file_path() -> PathBuf {
    xdg_config_home().join("skelz").join("config.toml")
}

pub fn xdg_config_home() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        let trimmed = xdg.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    let home = dirs_next::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".config")
}

pub fn default_solana_keypair_path() -> PathBuf {
    if let Ok(env_kp) = std::env::var("SOLANA_KEYPAIR") {
        if !env_kp.trim().is_empty() {
            return expand_tilde(Path::new(&env_kp));
        }
    }
    xdg_config_home().join("skelz").join("id.json")
}

pub fn default_cluster_rpc_url(cluster: &str) -> String {
    match cluster {
        "mainnet" | "mainnet-beta" => "https://api.mainnet-beta.solana.com".to_string(),
        "testnet" => "https://api.testnet.solana.com".to_string(),
        "localnet" | "local" => "http://127.0.0.1:8899".to_string(),
        _ => "https://api.devnet.solana.com".to_string(),
    }
}

pub fn expand_tilde(path: &Path) -> PathBuf {
    let p = path.to_string_lossy();
    if let Some(stripped) = p.strip_prefix("~/") {
        if let Some(home) = dirs_next::home_dir() {
            return home.join(stripped);
        }
    }
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_rpc_for_devnet() {
        assert_eq!(default_cluster_rpc_url("devnet"), "https://api.devnet.solana.com");
    }
}

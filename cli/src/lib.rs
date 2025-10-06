use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use solana_client::rpc_client::RpcClient;
use solana_sdk::hash::Hash;
use solana_sdk::instruction::Instruction;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{read_keypair_file, Signer};
use solana_sdk::transaction::Transaction;
use thiserror::Error;
use tracing::info;
use std::process::Command;

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

/// Structure for the Solana memo containing image signature information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageSignatureMemo {
    pub version: u32,
    pub artifact: ImageArtifact,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageArtifact {
    pub kind: String,
    pub digest: String,
}

/// Structure for the Solana proof payload to be uploaded as OCI artifact
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolanaProofPayload {
    pub network: String,
    pub tx_hash: String,
    pub tool: String,
}

/// OCI Artifact Manifest v1 structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactManifest {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub artifact_type: String,
    pub blobs: Vec<BlobDescriptor>,
    pub subject: Option<SubjectDescriptor>,
    pub annotations: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobDescriptor {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub digest: String,
    pub size: i64,
    pub annotations: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubjectDescriptor {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub digest: String,
    pub size: i64,
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

/// Extract digest from canonical image reference
pub fn extract_digest_from_reference(image_reference: &str) -> Result<String> {
    // Parse the digest from the canonical reference format: registry/repo@sha256:digest
    if let Some(start) = image_reference.find("@sha256:") {
        let digest_part = &image_reference[start + 1..]; // Remove the @
        Ok(digest_part.to_string())
    } else {
        Err(anyhow!("No digest found in image reference {}. Expected format: registry/repo@sha256:digest", image_reference))
    }
}

/// Sign a Docker image by publishing a memo on Solana
pub fn sign_docker_image(image_reference: &str, cfg: &SkelzConfig) -> Result<String> {
    // Extract the image digest from the canonical reference
    let digest = extract_digest_from_reference(image_reference)?;
    info!(%digest, "calculated image digest");
    
    // Create the signature memo
    let memo = ImageSignatureMemo {
        version: 1,
        artifact: ImageArtifact {
            kind: "oci-image".to_string(),
            digest: digest.clone(),
        },
    };
    
    // Serialize to JSON
    let memo_json = serde_json::to_string(&memo)
        .context("Failed to serialize memo to JSON")?;
    
    info!(%memo_json, "publishing signature memo");
    
    // Publish the memo on Solana
    let signature = sign_memo(&memo_json, cfg)?;
    
    info!(%signature, %image_reference, "image signed successfully");
    Ok(signature)
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

/// Sign an image with Solana and upload proof as OCI artifact
pub fn sign_image_with_oci(
    image_reference: &str,
    config: &SkelzConfig,
    username: &str,
    token: &str,
) -> Result<String> {
    info!("Signing image with OCI: {}", image_reference);
    
    // Sign the image on Solana first
    let signature = sign_docker_image(image_reference, config)?;
    info!(%signature, "image signed on Solana");
    
    // Create the Solana proof payload
    let payload = SolanaProofPayload {
        network: "solana-mainnet".to_string(),
        tx_hash: signature.clone(),
        tool: "skelz-cli@v1.0.0".to_string(),
    };
    
    let payload_json = json!(payload);
    let payload_bytes = serde_json::to_vec(&payload_json)
        .context("Failed to serialize payload to JSON")?;
    
    // Write payload to temporary file in current directory
    let signature_file = std::path::PathBuf::from("skelz-signature.json");
    std::fs::write(&signature_file, &payload_bytes)
        .context("Failed to write signature file")?;
    
    // Ensure image reference is for GHCR
    let ghcr_reference = if image_reference.starts_with("ghcr.io/") {
        image_reference.to_string()
    } else {
        // Extract repository and digest from the original reference
        let parts: Vec<&str> = image_reference.split('/').collect();
        if parts.len() >= 2 {
            let repo_with_tag = parts[1..].join("/");
            format!("ghcr.io/{}", repo_with_tag)
        } else {
            anyhow::bail!("Invalid image reference format: {}", image_reference);
        }
    };
    
    info!("Using GHCR reference: {}", ghcr_reference);
    
    // Use oras attach to attach the signature to the image
    let mut cmd = Command::new("oras");
    cmd.arg("attach")
        .arg("--artifact-type")
        .arg("application/vnd.skelz.proof.v1+json")
        .arg("--annotation")
        .arg(format!("org.opencontainers.artifact.created={}", chrono::Utc::now().to_rfc3339()))
        .arg("--annotation")
        .arg(format!("skelz.signature={}", signature))
        .arg("--annotation")
        .arg(format!("skelz.original-image={}", image_reference))
        .arg("--annotation")
        .arg("skelz.tool=skelz-cli@v1.0.0")
        .arg(&ghcr_reference)
        .arg(&signature_file);
    
    // Set authentication
    cmd.env("ORAS_USERNAME", username);
    cmd.env("ORAS_PASSWORD", token);
    
    info!("Running oras attach command...");
    let output = cmd.output()
        .context("Failed to execute oras command")?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        anyhow::bail!("oras attach failed:\nSTDOUT: {}\nSTDERR: {}", stdout, stderr);
    }
    
    info!("Successfully attached signature to image: {}", ghcr_reference);
    
    // Discover and display attached artifacts
    info!("Discovering attached artifacts...");
    let mut discover_cmd = Command::new("oras");
    discover_cmd.arg("discover")
        .arg(&ghcr_reference);
    
    // Set authentication for discover command
    discover_cmd.env("ORAS_USERNAME", username);
    discover_cmd.env("ORAS_PASSWORD", token);
    
    let discover_output = discover_cmd.output()
        .context("Failed to execute oras discover command")?;
    
    if discover_output.status.success() {
        let discover_stdout = String::from_utf8_lossy(&discover_output.stdout);
        info!("Attached artifacts:\n{}", discover_stdout);
        println!("Attached artifacts:\n{}", discover_stdout);
    } else {
        let discover_stderr = String::from_utf8_lossy(&discover_output.stderr);
        info!("Warning: Could not discover artifacts: {}", discover_stderr);
    }
    
    // Clean up temporary file
    let _ = std::fs::remove_file(&signature_file);
    
    info!(%signature, "signature attached successfully");
    Ok(signature)
}



/// Build an OCI Artifact Manifest v1
pub fn build_artifact_manifest(
    media_type: &str,
    subject_digest: &str,
    blob_digest: &str,
    blob_size: i64,
) -> ArtifactManifest {
    ArtifactManifest {
        media_type: "application/vnd.oci.artifact.manifest.v1+json".to_string(),
        artifact_type: media_type.to_string(),
        blobs: vec![BlobDescriptor {
            media_type: media_type.to_string(),
            digest: blob_digest.to_string(),
            size: blob_size,
            annotations: None,
        }],
        subject: Some(SubjectDescriptor {
            media_type: "application/vnd.oci.image.manifest.v1+json".to_string(),
            digest: subject_digest.to_string(),
            size: 0, // We don't have the actual manifest size
        }),
        annotations: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_rpc_for_devnet() {
        assert_eq!(default_cluster_rpc_url("devnet"), "https://api.devnet.solana.com");
    }
}

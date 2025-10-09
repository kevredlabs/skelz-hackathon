use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::process::Command;
use std::collections::HashMap;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{read_keypair_file, Signer};
use thiserror::Error;
use tracing::{info, error};
use anchor_client::{
    solana_sdk::{
        commitment_config::CommitmentConfig,
        system_program,
        pubkey::Pubkey as AnchorPubkey,
        signature::Keypair,
    },
    Client, Cluster,
};
use anchor_lang::prelude::*;
use std::rc::Rc;
use sha2::{Sha256, Digest};

// Declare the program using the IDL (exactly like in the test)
declare_program!(skelz);
use skelz::{accounts::Signature, client::accounts, client::args};

// Define the program ID
const SKELZ_PROGRAM_ID: &str = "4uw8DwTRdUMwGmbNrK5GZ5kgdVtco4aUaTGDnEUBrYKt";


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
    pub ghcr_user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ghcr_token: Option<String>,
}


/// Structure for the Solana proof payload to be uploaded as OCI artifact
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolanaProofPayload {
    pub network: String,
    pub tx_hash: String,
    pub tool: String,
}

/// Structure for OCI artifact metadata with annotations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OciArtifact {
    pub reference: String,
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub digest: String,
    pub size: u64,
    pub annotations: HashMap<String, String>,
    #[serde(rename = "artifactType")]
    pub artifact_type: String,
    pub referrers: Vec<OciArtifact>,
}

/// Structure for OCI discover response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OciDiscoverResponse {
    pub reference: String,
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub digest: String,
    pub size: u64,
    pub referrers: Vec<OciArtifact>,
}


impl Default for SkelzConfig {
    fn default() -> Self {
        Self {
            cluster: "devnet".to_string(),
            rpc_url: default_cluster_rpc_url("devnet"),
            keypair_path: default_solana_keypair_path(),
            commitment: "confirmed".to_string(),
            ghcr_user: None,
            ghcr_token: None,
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

pub fn resolve_ghcr_credentials(cfg: &SkelzConfig) -> Result<(String, String)> {
    let env_user = std::env::var("GHCR_USER").ok().filter(|v| !v.trim().is_empty());
    let env_token = std::env::var("GHCR_TOKEN").ok().filter(|v| !v.trim().is_empty());

    let user = env_user.or_else(|| cfg.ghcr_user.clone());
    let token = env_token.or_else(|| cfg.ghcr_token.clone());

    match (user, token) {
        (Some(u), Some(t)) => Ok((u, t)),
        _ => Err(anyhow!(
            "GHCR credentials not found. Set GHCR_USER/GHCR_TOKEN or set ghcr_user/ghcr_token in config.toml"
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


pub fn get_config_value(cfg: &SkelzConfig, key: &str) -> Result<String> {
    match key {
        "cluster" => Ok(cfg.cluster.clone()),
        "rpc_url" => Ok(cfg.rpc_url.clone()),
        "keypair_path" => Ok(cfg.keypair_path.display().to_string()),
        "commitment" => Ok(cfg.commitment.clone()),
        "ghcr_user" => Ok(cfg.ghcr_user.clone().unwrap_or_default()),
        // Do not print secrets in clear text
        "ghcr_token" => Ok("<redacted>".to_string()),
        _ => Err(SkelzError::UnknownConfigKey(key.to_string()).into()),
    }
}

pub fn set_config_value(cfg: &mut SkelzConfig, key: &str, value: &str) -> Result<()> {
    match key {
        "cluster" => cfg.cluster = value.to_string(),
        "rpc_url" => cfg.rpc_url = value.to_string(),
        "keypair_path" => cfg.keypair_path = expand_tilde(Path::new(value)),
        "commitment" => cfg.commitment = value.to_string(),
        "ghcr_user" => cfg.ghcr_user = Some(value.to_string()),
        "ghcr_token" => cfg.ghcr_token = Some(value.to_string()),
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

/// Sign a Docker image using the Anchor program
pub fn sign_docker_image_with_anchor(image_reference: &str, cfg: &SkelzConfig) -> Result<String> {
    info!("Signing image with Anchor program: {}", image_reference);
    
    // Extract the image digest from the canonical reference
    let digest = extract_digest_from_reference(image_reference)?;
    info!(%digest, "calculated image digest");
    
    // Use the hardcoded program ID
    let program_id = AnchorPubkey::from_str(SKELZ_PROGRAM_ID)
        .context("Invalid program ID format")?;
    
    info!("Using program ID: {}", program_id);
    
    // Load the keypair
    let payer = read_keypair_file(&cfg.keypair_path)
        .map_err(|e| anyhow!("read keypair at {}: {}", cfg.keypair_path.display(), e))?;
    
    // Create the Anchor client
    let cluster = match cfg.cluster.as_str() {
        "mainnet" | "mainnet-beta" => Cluster::Mainnet,
        "testnet" => Cluster::Testnet,
        "localnet" | "local" => Cluster::Localnet,
        _ => Cluster::Devnet,
    };
    
    info!("Using cluster: {:?}", cluster);
    info!("RPC URL: {}", cfg.rpc_url);
    info!("Payer: {}", payer.pubkey());
    
    let provider = Client::new_with_options(
        cluster,
        Rc::new(payer),
        CommitmentConfig::confirmed(),
    );
    
    let program = provider.program(program_id)?;
    
    // Derive the PDA for this signature
    // Hash the digest to create a shorter seed (32 bytes max)
    let mut hasher = sha2::Sha256::new();
    hasher.update(digest.as_bytes());
    let digest_hash = hasher.finalize();
    
    info!("Digest hash for PDA: {}", hex::encode(digest_hash));
    
    let (signature_pda, _bump) = AnchorPubkey::find_program_address(
        &[b"signature", &digest_hash],
        &program_id,
    );
    
    info!("Signature PDA: {}", signature_pda);
    
    // Use Anchor's request builder exactly like in the test
    info!("Sending transaction with accounts:");
    info!("  signer: {}", program.payer());
    info!("  pda: {}", signature_pda);
    info!("  system_program: {}", system_program::ID);
    info!("  digest: {}", digest);
    
    let result = program
        .request()
        .accounts(accounts::WriteSignature {
            signer: program.payer(),
            signature: signature_pda,
            system_program: system_program::ID,
        })
        .args(args::WriteSignature {
            digest: digest.clone(),
        })
        .send();
    
    let signature = match result {
        Ok(sig) => sig,
        Err(e) => {
            error!("Transaction failed: {:?}", e);
            if let anchor_client::ClientError::ProgramError(program_error) = &e {
                error!("Program error: {:?}", program_error);
            }
            return Err(e.into());
        }
    };
    
    info!(%signature, %image_reference, "image signed successfully with Anchor program");
    Ok(signature.to_string())
}



/// Sign an image with Solana and upload proof as OCI artifact
pub fn sign_image_with_oci(
    image_reference: &str,
    config: &SkelzConfig,
    username: &str,
    token: &str,
) -> Result<String> {
    info!("Signing image with OCI: {}", image_reference);
    
    // Sign the image on Solana using the Anchor program
    let signature = sign_docker_image_with_anchor(image_reference, config)?;
    info!(%signature, "image signed on Solana with Anchor program");
    
    // Create the Solana proof payload
    let payload = SolanaProofPayload {
        network: "solana-devnet".to_string(),
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

/// Discover OCI artifacts attached to an image
pub fn discover_oci_artifacts(
    image_reference: &str,
    username: &str,
    token: &str,
) -> Result<Vec<OciArtifact>> {
    info!("Discovering OCI artifacts for image: {}", image_reference);
    
    let mut cmd = Command::new("oras");
    cmd.arg("discover")
        .arg("--format")
        .arg("json")
        .arg(image_reference);
    
    // Set authentication
    cmd.env("ORAS_USERNAME", username);
    cmd.env("ORAS_PASSWORD", token);
    
    let output = cmd.output()
        .context("Failed to execute oras discover command")?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        anyhow::bail!("oras discover failed:\nSTDOUT: {}\nSTDERR: {}", stdout, stderr);
    }
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    info!("ORAS discover output: {}", stdout);
    
    // Parse the JSON response
    let discover_response: OciDiscoverResponse = serde_json::from_str(&stdout)
        .context("Failed to parse oras discover JSON response")?;
    
    info!("Found {} artifacts", discover_response.referrers.len());
    Ok(discover_response.referrers)
}

/// Get the latest Skelz artifact from a list of OCI artifacts
pub fn get_latest_skelz_artifact<'a>(artifacts: &'a [OciArtifact], expected_image: &str) -> Result<&'a OciArtifact> {
    // Filter for Skelz artifacts (those with skelz.signature annotation and correct image)
    let mut skelz_artifacts: Vec<&OciArtifact> = artifacts
        .iter()
        .filter(|artifact| {
            artifact.annotations.contains_key("skelz.signature") &&
            artifact.artifact_type == "application/vnd.skelz.proof.v1+json" &&
            artifact.annotations.get("skelz.original-image") == Some(&expected_image.to_string())
        })
        .collect();
    
    if skelz_artifacts.is_empty() {
        anyhow::bail!("No Skelz signature artifacts found for image: {}", expected_image);
    }
    
    // Sort by creation time (most recent first)
    skelz_artifacts.sort_by(|a, b| {
        let time_a = a.annotations.get("org.opencontainers.artifact.created")
            .or_else(|| a.annotations.get("org.opencontainers.image.created"))
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .unwrap_or_else(|| chrono::DateTime::parse_from_rfc3339("1970-01-01T00:00:00Z").expect("Invalid fallback date"));
        
        let time_b = b.annotations.get("org.opencontainers.artifact.created")
            .or_else(|| b.annotations.get("org.opencontainers.image.created"))
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .unwrap_or_else(|| chrono::DateTime::parse_from_rfc3339("1970-01-01T00:00:00Z").expect("Invalid fallback date"));
        
        time_b.cmp(&time_a) // Most recent first
    });
    
    info!("Found {} Skelz artifacts for image, using the most recent one", skelz_artifacts.len());
    Ok(skelz_artifacts[0])
}

/// Simple verification function that only checks OCI artifacts (without Solana verification)
pub fn verify_oci_artifacts(
    image_reference: &str,
    username: &str,
    token: &str,
) -> Result<()> {
    info!("Starting OCI artifact verification for: {}", image_reference);
    
    // Step 1: Validate image reference format
    if !image_reference.contains("@sha256:") {
        anyhow::bail!("Image reference must be canonical with digest (e.g., ghcr.io/username/repo@sha256:abc123...)");
    }
    
    if !image_reference.starts_with("ghcr.io") {
        anyhow::bail!("Only GitHub Container Registry is supported. Use format: ghcr.io/username/repo@sha256:abc123...");
    }
    
    // Step 2: Discover OCI artifacts
    let artifacts = discover_oci_artifacts(image_reference, username, token)?;
    
    // Step 3: Get the latest Skelz artifact
    let skelz_artifact = get_latest_skelz_artifact(&artifacts, image_reference)?;
    
    // Step 4: Display artifact information
    info!("✅ OCI artifact verification successful!");
    println!("✅ OCI artifact verification successful!");
    println!("   Image: {}", image_reference);
    println!("   Artifact reference: {}", skelz_artifact.reference);
    println!("   Artifact digest: {}", skelz_artifact.digest);
    println!("   Media type: {}", skelz_artifact.media_type);
    println!("   Artifact type: {}", skelz_artifact.artifact_type);
    println!("   Size: {} bytes", skelz_artifact.size);
    
    // Display annotations
    println!("   Annotations:");
    for (key, value) in &skelz_artifact.annotations {
        println!("     {}: {}", key, value);
    }
    
    Ok(())
}


/// Verify signature using PDA-based system with Anchor
pub fn verify_signature(
    program: &anchor_client::Program<Rc<Keypair>>,
    digest: &str,
    expected_signer: &str,
) -> Result<()> {
    info!("Verifying signature for digest: {}", digest);
    
    // Step 1: Calculate PDA with the same seed as the program
    let mut hasher = Sha256::new();
    hasher.update(digest.as_bytes());
    let digest_hash = hasher.finalize();
    
    let (signature_pda, _bump) = Pubkey::find_program_address(
        &[b"signature", &digest_hash[..]],
        &program.id(),
    );
    
    info!("Calculated PDA: {}", signature_pda);
    
    // Step 2: Check if the account exists on Solana using Anchor IDL
    let signature_account: Signature = program.account::<Signature>(signature_pda)
        .map_err(|e| anyhow!("Signature account not found: {}. This means the image was not signed or not exists.", e))?;
    
    // Step 3: Verify the account data matches expectations
    if signature_account.digest != digest {
        anyhow::bail!(
            "Digest mismatch: expected {}, got {}",
            digest,
            signature_account.digest
        );
    }
    
    // Step 4: Verify the signer matches expected signer
    let expected_pubkey = Pubkey::from_str(expected_signer)
        .context("Invalid expected signer public key format")?;
    
    if signature_account.signer != expected_pubkey {
        anyhow::bail!(
            "Signer mismatch: expected {}, got {}",
            expected_pubkey,
            signature_account.signer
        );
    }
    
    info!("✅ Signature verification successful!");
    println!("✅ Signature verification successful!");
    println!("   - Digest: {}", signature_account.digest);
    println!("   - Signer: {}", signature_account.signer);
    println!("   - PDA: {}", signature_pda);
    
    Ok(())
}

/// Complete verification function using PDA-based system
pub fn verify_image_signature(
    image_reference: &str,
    expected_signer: &str,
    config: &SkelzConfig,
    _username: &str,
    _token: &str,
) -> Result<()> {
    info!("Starting PDA-based image signature verification for: {}", image_reference);
    
    // Step 1: Validate image reference format
    if !image_reference.contains("@sha256:") {
        anyhow::bail!("Image reference must be canonical with digest (e.g., ghcr.io/username/repo@sha256:abc123...)");
    }
    
    if !image_reference.starts_with("ghcr.io") {
        anyhow::bail!("Only GitHub Container Registry is supported. Use format: ghcr.io/username/repo@sha256:abc123...");
    }
    
    // Step 2: Extract digest from image reference
    let digest = extract_digest_from_reference(image_reference)?;
    info!("Extracted digest: {}", digest);
    
    // Step 3: Configure Anchor program client using config keypair
    let payer = read_keypair_file(&config.keypair_path)
        .map_err(|e| anyhow!("read keypair at {}: {}", config.keypair_path.display(), e))?;
    let provider = Client::new_with_options(
        Cluster::Devnet,
        Rc::new(payer),
        CommitmentConfig::confirmed(),
    );
    let program = provider.program(skelz::ID)?;
    
    // Step 4: Verify signature using PDA with Anchor IDL
    verify_signature(&program, &digest, expected_signer)?;
    
    info!("✅ Complete image signature verification successful!");
    println!("✅ Complete image signature verification successful!");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_rpc_for_devnet() {
        assert_eq!(default_cluster_rpc_url("devnet"), "https://api.devnet.solana.com");
    }
}
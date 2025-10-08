use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::process::Command;
use std::collections::HashMap;

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

/// Fetch a Solana transaction by signature and return signer pubkey and memo
pub fn fetch_solana_transaction_with_memo(
    tx_signature: &str,
    rpc_url: &str,
) -> Result<(String, ImageSignatureMemo)> {
    info!("Fetching Solana transaction: {}", tx_signature);
    
    // Use tokio runtime for async operations
    let rt = tokio::runtime::Runtime::new()
        .context("Failed to create tokio runtime")?;
    
    rt.block_on(async {
        let client = solana_client::nonblocking::rpc_client::RpcClient::new_with_commitment(
            rpc_url.to_string(),
            solana_sdk::commitment_config::CommitmentConfig::confirmed(),
        );
        
        // Parse the signature
        let signature = solana_sdk::signature::Signature::from_str(tx_signature)
            .context("Invalid transaction signature format")?;
        
        // Configure the request
        let config = solana_client::rpc_config::RpcTransactionConfig {
            commitment: solana_sdk::commitment_config::CommitmentConfig::finalized().into(),
            encoding: Some(solana_transaction_status::UiTransactionEncoding::Base64),
            max_supported_transaction_version: Some(0),
        };
        
        // Fetch the transaction
        let tx = client
            .get_transaction_with_config(&signature, config)
            .await
            .context("Failed to fetch transaction from Solana RPC")?;
        
        // Extract the signer and memo from the transaction
        let (signer_pubkey, memo_data) = match &tx.transaction.transaction {
            solana_transaction_status::EncodedTransaction::Binary(encoded_tx, _) => {
                // Decode the base64 transaction
                let tx_bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded_tx)
                    .context("Failed to decode transaction from base64")?;
                
                // Parse the transaction to get the signer and memo
                let transaction: solana_sdk::transaction::VersionedTransaction = 
                    bincode::deserialize(&tx_bytes)
                        .context("Failed to deserialize transaction")?;
                
                // The first signature corresponds to the first account (fee payer/signer)
                let signer = transaction.message.static_account_keys()[0];
                let signer_pubkey = signer.to_string();
                
                // Extract memo data from the first instruction (memo instruction)
                let memo_data = if let Some(instruction) = transaction.message.instructions().first() {
                    // The memo instruction data is the memo content
                    String::from_utf8(instruction.data.clone())
                        .context("Memo data is not valid UTF-8")?
                } else {
                    anyhow::bail!("No instructions found in transaction");
                };
                
                (signer_pubkey, memo_data)
            }
            _ => anyhow::bail!("Unexpected transaction encoding format"),
        };
        
        println!("Signer pubkey: {}", signer_pubkey);
        println!("Memo data: {}", memo_data);
        
        // Parse the memo to extract image information
        let memo: ImageSignatureMemo = serde_json::from_str(&memo_data)
            .context("Failed to parse memo as ImageSignatureMemo")?;
        
        println!("Parsed memo: {:#?}", memo);
        
        info!("Successfully fetched transaction from slot {}", tx.slot);
        Ok((signer_pubkey, memo))
    })
}

/// Verify that the transaction was signed by the expected signer
pub fn verify_transaction_signer(
    signer_pubkey: &str,
    expected_signer: &str,
) -> Result<()> {
    info!("Verifying transaction signer: {}", expected_signer);
    
    let expected_pubkey = Pubkey::from_str(expected_signer)
        .context("Invalid expected signer public key format")?;
    
    let signer_pubkey = Pubkey::from_str(signer_pubkey)
        .context("Invalid signer public key in transaction")?;
    
    if signer_pubkey != expected_pubkey {
        anyhow::bail!(
            "Transaction signer mismatch: expected {}, got {}",
            expected_pubkey,
            signer_pubkey
        );
    }
    
    info!("Transaction signer verification successful");
    Ok(())
}

/// Extract and verify the image digest from the Solana memo
pub fn verify_image_digest(
    memo: &ImageSignatureMemo,
    expected_image_reference: &str,
) -> Result<()> {
    info!("Verifying image digest for: {}", expected_image_reference);
    
    // Extract expected digest from image reference
    let expected_digest = extract_digest_from_reference(expected_image_reference)?;
    info!("Expected digest: {}", expected_digest);
    
    // Verify the digest matches
    if memo.artifact.digest != expected_digest {
        anyhow::bail!(
            "Image digest mismatch: expected {}, got {}",
            expected_digest,
            memo.artifact.digest
        );
    }
    
    // Verify the artifact kind
    if memo.artifact.kind != "oci-image" {
        anyhow::bail!(
            "Invalid artifact kind: expected 'oci-image', got '{}'",
            memo.artifact.kind
        );
    }
    
    info!("Image digest verification successful");
    Ok(())
}

/// Complete verification function that checks both OCI artifacts and Solana transaction
pub fn verify_image_signature(
    image_reference: &str,
    expected_signer: &str,
    config: &SkelzConfig,
    username: &str,
    token: &str,
) -> Result<()> {
    info!("Starting complete image signature verification for: {}", image_reference);
    
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
    
    // Step 4: Extract transaction signature from annotations
    let tx_signature = skelz_artifact.annotations
        .get("skelz.signature")
        .ok_or_else(|| anyhow!("No skelz.signature annotation found in artifact"))?;
    
    info!("Found transaction signature: {}", tx_signature);
    
    // Step 5: Fetch the Solana transaction and extract signer + memo
    let (signer_pubkey, memo) = fetch_solana_transaction_with_memo(tx_signature, &config.rpc_url)?;
    
    // Step 6: Verify the transaction signer
    verify_transaction_signer(&signer_pubkey, expected_signer)?;
    
    // Step 7: Verify the image digest
    verify_image_digest(&memo, image_reference)?;
    
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
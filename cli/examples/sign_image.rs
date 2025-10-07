use anyhow::Result;
use skelz::{load_config_with_overrides, sign_docker_image, SkelzConfig};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();
    
    // Load configuration
    let config = load_config_with_overrides(None, None)?;
    
    // Example: Sign a Docker image
    let image_reference = "nginx:latest";
    println!("Signing Docker image: {}", image_reference);
    
    match sign_docker_image(image_reference, &config) {
        Ok(signature) => {
            println!("✅ Image signed successfully!");
            println!("Transaction signature: {}", signature);
            println!("The signature memo contains:");
            println!("- Image digest");
            println!("- Signature timestamp");
            println!("- Signer public key");
        }
        Err(e) => {
            println!("❌ Failed to sign image: {}", e);
            println!("Make sure:");
            println!("1. Docker is running");
            println!("2. The image is pulled locally");
            println!("3. Solana configuration is correct");
        }
    }
    
    Ok(())
}

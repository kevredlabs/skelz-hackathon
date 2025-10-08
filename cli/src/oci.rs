use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// OCI manifest with annotations for Solana signatures
/// This structure is kept for potential future use with OCI manifests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OciManifest {
    pub schema_version: u32,
    pub media_type: String,
    pub config: OciDescriptor,
    pub layers: Vec<OciDescriptor>,
    pub annotations: Option<HashMap<String, String>>,
}

/// OCI descriptor for manifests and layers
/// This structure is kept for potential future use with OCI descriptors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OciDescriptor {
    pub media_type: String,
    pub size: u64,
    pub digest: String,
    pub annotations: Option<HashMap<String, String>>,
}

// Note: All the OCI manipulation functions have been removed as they are now
// handled by the oci-client crate in lib.rs. The structures above are kept
// for potential future use if we need to work with OCI manifests directly.
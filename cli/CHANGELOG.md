# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Initial release of Skelz CLI
- Docker image signing with Solana blockchain signatures
- Image signature verification against Solana blockchain
- GitHub Container Registry (GHCR) integration
- Configuration management with TOML files
- Support for multiple Solana clusters (devnet, testnet, mainnet-beta)
- OCI artifact upload and retrieval for signature proofs
- Command-line interface with subcommands: config, sign, verify, registry

### Features
- `skelz config init` - Initialize configuration file
- `skelz config get/set` - Manage configuration values
- `skelz sign` - Sign Docker images with Solana signatures
- `skelz verify` - Verify image signatures
- `skelz registry login` - Authenticate with GHCR

### Configuration
- XDG-compliant configuration file location
- Environment variable overrides (SOLANA_RPC_URL, SOLANA_KEYPAIR, GHCR_USER, GHCR_TOKEN)
- Support for custom RPC URLs and keypair paths

## [0.1.0] - 2025-01-27

### Added
- Initial MVP release for hackathon
- Basic CLI functionality
- Solana program integration
- OCI registry support

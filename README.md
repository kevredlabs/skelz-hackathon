# Skelz – Bringing trust to the software supply chain: sign, verify, and secure container images on Solana

## Pitch

- **Problem**: The software supply chain is fragile (compromised dependencies/images, centralized registries that can be corrupted or censored). How can we guarantee that the deployed image is exactly the one that was built and approved?
- **Solution**: An on-chain (Solana) registry of image digests and signatures; attestations (SBOM, provenance) stored on IPFS/Arweave; immutable policies; automatic verification in Kubernetes via an Admission Controller.
- **Workflow**: CI builds → Cosign signs → On-chain publication (digest, signatures, IPFS CIDs) → Admission Controller compares digest/signatures/policies → ALLOW/DENY.
- **Benefits**: Security, transparency, compliance, interoperability (Sigstore/Cosign), censorship resistance.

## MVP scope (hackathon)

- Solana smart contract (registry of digests, signatures, CIDs, policies)
- CLI `publish`/`verify` (Rust)
- K8s Admission Controller (Rust) for ALLOW/DENY
- E2E demo (KinD): build → sign → publish → deploy

## Planned structure

```text
contracts/              # Solana program (Anchor/Rust)
cli/                    # CLI publish/verify (Rust)
admission-controller/   # K8s webhook (Rust)
sdk/                    # Shared clients (Rust)
infra/                  # K8s manifests, Helm, KinD
docs/                   # Docs, diagrams, ADRs
examples/               # E2E flows
test/                   # Integration/E2E tests
```

## Conventions

- Short-lived branches `feat/*`, `fix/*`, merges into `main` (protected)
- Commits: Conventional Commits (`feat:`, `fix:`, `chore:` …)
- Formatting/lints: Rust (`rustfmt` / `clippy`)
- Hooks: `pre-commit`

## Prerequisites (dev)

- Docker, Cosign, Rust/Cargo, Solana CLI, Kind/kubectl

## Quick start

Run all commands from inside `cli/` (the Makefile lives there):

```bash
cd cli
make setup
make build
make test
```

### CLI

See `cli/README.md` for the `skelz` CLI usage (`config`, `sign`, `verify`).

## CI/CD and image signing

The project includes two GitHub Actions workflows for container image signing:

### Legacy Signing (OIDC-based)

- `legacy-signing-and-verifying.yml`: Signs images using GitHub OIDC tokens
- Automatically builds the demo image on each push
- Signs with Cosign using GitHub's OIDC infrastructure
- Publishes to GitHub Container Registry (ghcr.io)

### Private Key Signing

- `private-key-signing-and-verifying.yml`: Signs images using private keys
- Provides more control over the signing process
- Requires configuration of GitHub secrets (`COSIGN_PRIVATE_KEY`, `COSIGN_PASSWORD`)
- See `docs/private-key-signing-setup.md` for detailed setup instructions

Both workflows support the amd64 architecture and the image is available at: `ghcr.io/kevredlabs/skelz:latest`

## License

MIT — see `LICENSE`.

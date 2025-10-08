Skelz CLI

Hackathon-ready CLI with three commands: `config`, `sign`, `verify`.

## Makefile

This directory ships its own `Makefile`. Run commands from here:

- `make build`
- `make test`
- `make lint`

## Build & install

```
# With Makefile (recommended)
make build
make test
make lint

# Run
cargo build --release
cargo run -- --help

# Install locally (puts `skelz` in ~/.cargo/bin)
cargo install --path .
skelz --help
```

Toolchain:

- CLI pins Rust to `stable` via `cli/rust-toolchain.toml`.
- If needed, update toolchain with:
```
rustup update stable
rustup default stable
```

## Global flags

- `-v` / `-vv`: increase verbosity (uses `tracing` under the hood)

## Commands

### config
Manage configuration.

Usage:
```
skelz config <COMMAND>
```

Subcommands:
- `init`: generate a config file
- `get`: get a config value or print full config
- `set`: set a config value

Keys:
- `cluster`, `rpc_url`, `keypair_path`, `commitment`
- `ghcr_user`, `ghcr_token` (optional, only if you can't use env)

Examples:
```
# Init default config (XDG path)
skelz config init

# Init with custom path
skelz config init --output ./skelz.toml --force

# Get current rpc_url
skelz config get rpc_url

# Set rpc_url
skelz config set rpc_url https://api.devnet.solana.com

# Set cluster (will not auto-update rpc_url unless you pass --rpc-url at init)
skelz config set cluster devnet

# Print full config (TOML)
skelz config get
```

### sign
Publish a text memo on Solana (Memo v2 program) with your fee payer keypair.

Flags:
- `--rpc-url <URL>`
- `--keypair <PATH>`

Environment overrides (if flags are not provided):
- `SOLANA_RPC_URL`
- `SOLANA_KEYPAIR`

Examples:
```
# Devnet
skelz -v sign "hello skelz" \
  --rpc-url https://api.devnet.solana.com \
  --keypair ~/.config/solana/id.json

# Local validator
skelz -vv sign "local memo" \
  --rpc-url http://127.0.0.1:8899 \
  --keypair ~/.config/solana/id.json
```
Output:
- Prints `Signature=<SIGNATURE>` upon success

Notes:
- Memo program id (v2): `MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr`

### verify
Placeholder (no-op for now).

## Environment variables
- `SOLANA_RPC_URL`: RPC endpoint
- `SOLANA_KEYPAIR`: path to fee payer keypair (default: `~/.config/skelz/id.json`)
- `GHCR_USER`, `GHCR_TOKEN`: preferred source for GitHub Container Registry creds

Resolution order for GHCR credentials:
1. Environment variables `GHCR_USER` and `GHCR_TOKEN` (recommended)
2. Fallback to config TOML keys `ghcr_user` and `ghcr_token` if set

Example TOML snippet (only if you can't use env):
```
# ... other keys ...
ghcr_user = "my-github-username"
ghcr_token = "<github-personal-access-token>"
```

## Defaults
- Config path: XDG `~/.config/skelz/config.toml`
- Cluster default: `devnet` (`https://api.devnet.solana.com`)
- Keypair default: `~/.config/skelz/id.json`

### registry
Work with GitHub Container Registry (GHCR) credentials.

Subcommands:
- `login`: perform `docker login` using env/TOML credentials

Env-first resolution:
1. `GHCR_USER`, `GHCR_TOKEN`
2. Fallback to TOML `ghcr_user`, `ghcr_token`

Usage:
```
skelz registry login
# or specify a different registry
skelz registry login --registry ghcr.io
# override username
skelz registry login --username my-github-username
```

### sign-image
Sign a Docker image with Solana signature and upload to GHCR.

Flags:
- `--rpc-url <URL>`
- `--keypair <PATH>`
- `--ghcr-user <USERNAME>` (optional, uses GHCR_USER env var if not provided)
- `--ghcr-token <TOKEN>` (optional, uses GHCR_TOKEN env var if not provided)

Examples:
```
# Sign and upload to GHCR
skelz sign-image ghcr.io/username/repo@sha256:abc123... \
  --ghcr-user my-github-username \
  --ghcr-token ghp_xxxxxxxxxxxx

# Using environment variables
export GHCR_USER=my-github-username
export GHCR_TOKEN=ghp_xxxxxxxxxxxx
skelz sign-image ghcr.io/username/repo@sha256:abc123...
```

Output:
- Prints `Image Signature=<SIGNATURE>` upon success
- Uploads Solana proof as OCI artifact to GHCR

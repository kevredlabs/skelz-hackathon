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

## Defaults
- Config path: XDG `~/.config/skelz/config.toml`
- Cluster default: `devnet` (`https://api.devnet.solana.com`)
- Keypair default: `~/.config/skelz/id.json`

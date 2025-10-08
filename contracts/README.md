## Skelz Contracts (Anchor minimal hello)

Minimal Anchor program with a single `hello` instruction that logs a message ("Hello, Skelz!"). No state.

### Prerequisites
- Rust toolchain (2021), cargo
- Solana CLI (`solana --version`)
- A funded keypair on the target cluster (for devnet: airdrop)

### Key environment variables
- `ANCHOR_WALLET`: path to the signer keypair JSON. Overridable (defaults to `$(HOME)/.solana/phantom_account.json`).
- `CLUSTER`: one of `localnet`, `devnet`, `testnet`, `mainnet` (default: `devnet`).

Example set up (macOS/Linux):
```bash
export ANCHOR_WALLET=$HOME/.config/solana/id.json   # or any keypair path
solana config set --keypair $ANCHOR_WALLET
solana config set --url https://api.devnet.solana.com
solana airdrop 2  # devnet only; may need to retry
```

### Build
From this `contracts/` directory:
```bash
make build
```

### Deploy
```bash
# Deploy to the configured cluster (default devnet)
make deploy

# After first deploy, update declare_id! in the program automatically
make fix-id

# Optional: force deploy via solana CLI
make force-deploy
```

You can override variables per-invocation:
```bash
make CLUSTER=devnet ANCHOR_WALLET=$HOME/.config/solana/id.json deploy
```

### Redeploy quick flow
```bash
make redeploy   # force-deploy + fix-id + test
```

### Local validator (localnet)
Start a local validator in one terminal:
```bash
make localnet-up
```

In another terminal, target localnet and deploy:
```bash
make CLUSTER=localnet build deploy fix-id
make airdrop      # funds default key on localnet
```

Stop/reset:
```bash
make localnet-down
make reset-localnet
```

### Useful targets
```bash
make help          # show all targets
make explorer      # open explorer URL for current program id + cluster
make logs          # tail local validator logs
make watch         # auto build+deploy on file changes (requires cargo-watch)
```

### Program interface
- Program name: `skelz`
- Instruction: `hello()`
- Accounts: one signer required (any signer is acceptable)
- Behavior: logs `Hello, Skelz!` and returns

Client note: invoke `hello` via Anchor client/IDL or custom client with the Anchor discriminator + no args.

### Troubleshooting
- Program ID mismatch: run `make fix-id` after the first deploy.
- Not enough funds: airdrop (devnet) or fund the keypair.
- Wrong cluster: set `CLUSTER` or `solana config set --url ...`.
- Wallet path wrong: set `ANCHOR_WALLET` or update your Solana config.



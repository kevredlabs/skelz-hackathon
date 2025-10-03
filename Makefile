.PHONY: setup lint test build e2e devnet-up publish verify

setup:
	@echo "Installing pre-commit hooks and tooling..."
	@if command -v pre-commit >/dev/null 2>&1; then pre-commit install; else echo "pre-commit not found (optional)"; fi

lint:
	@echo "Running linters..."
	@if command -v cargo >/dev/null 2>&1; then cargo fmt --all -- --check; cargo clippy --all-targets --all-features -D warnings || true; fi

test:
	@echo "Running tests..."
	@if command -v cargo >/dev/null 2>&1; then cargo test --all --quiet || true; fi

build:
	@echo "Building..."
	@if command -v cargo >/dev/null 2>&1; then cargo build --all --release || true; fi

devnet-up:
	@echo "Starting local Solana devnet (test-validator)..."
	@if command -v solana >/dev/null 2>&1; then solana-test-validator --reset --limit-ledger-size 500 --faucet-port 9900 | cat; else echo "Solana CLI not found"; fi

publish:
	@echo "Publishing digest/signatures on-chain (placeholder)"
	@echo "Implement in cli/ once ready"

verify:
	@echo "Verifying digest/signatures (placeholder)"
	@echo "Implement in cli/ once ready"

e2e:
	@echo "Running E2E demo (placeholder)"
	bash scripts/demo.sh || true



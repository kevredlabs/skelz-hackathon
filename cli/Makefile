.PHONY: lint test build install publish

lint:
	@echo "Running linters..."
	@if command -v cargo >/dev/null 2>&1; then cargo fmt -- --check; cargo clippy --all-targets --all-features -D warnings || true; fi

test:
	@echo "Running tests..."
	@if command -v cargo >/dev/null 2>&1; then cargo test --quiet || true; fi

build:
	@echo "Building..."
	@if command -v cargo >/dev/null 2>&1; then cargo build --release || true; fi

install:
	@echo "Installing skelz binary locally..."
	@if command -v cargo >/dev/null 2>&1; then cargo install --path . || true; else echo "Cargo not found"; fi

build-install:
	@echo "Building and installing skelz..."
	@if command -v cargo >/dev/null 2>&1; then cargo build --release && cargo install --path . || true; else echo "Cargo not found"; fi

publish:
	@echo "Publishing digest/signatures on-chain (placeholder)"
	@echo "Implement in cli/ once ready"

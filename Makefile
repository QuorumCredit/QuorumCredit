.PHONY: build test generate-sdk check-sdk deploy-testnet deploy-mainnet indexer indexer-test

# ── Config ────────────────────────────────────────────────────────────────────

CONTRACT_DIR := .
WASM_TARGET  := wasm32v1-none
WASM_FILE    := $(CONTRACT_DIR)/target/$(WASM_TARGET)/release/quorum_credit.wasm

# ── Targets ───────────────────────────────────────────────────────────────────

## Compile the contract (native + WASM release build)
build:
	cd $(CONTRACT_DIR) && cargo build --target $(WASM_TARGET) --release

## Generate contract-parity SDK clients from the compiled WASM spec
generate-sdk: build
	cargo run -p sdkgen --bin contract_spec_extractor -- \
		--wasm $(WASM_FILE) \
		--spec-json sdk/contract_spec.json \
		--typescript sdk/typescript/src/client.ts \
		--python sdk/python/quorum_credit/client.py

## Verify generated SDK clients are current
check-sdk: build
	cargo run -p sdkgen --bin contract_spec_extractor -- \
		--wasm $(WASM_FILE) \
		--spec-json sdk/contract_spec.json \
		--typescript sdk/typescript/src/client.ts \
		--python sdk/python/quorum_credit/client.py \
		--check

## Run the full test suite
test:
	cd $(CONTRACT_DIR) && cargo test

## Deploy to Stellar testnet
deploy-testnet:
	stellar contract deploy \
		--wasm $(WASM_FILE) \
		--network testnet \
		--source $(DEPLOYER_SECRET_KEY)

## Deploy to Stellar mainnet — requires interactive confirmation
deploy-mainnet:
	@echo "WARNING: You are about to deploy to MAINNET."
	@read -p "Are you sure you want to deploy to MAINNET? [y/N]: " confirm && \
		[ "$${confirm:-N}" = "y" ] || [ "$${confirm:-N}" = "Y" ] || \
		(echo "Deployment aborted."; exit 1)
	stellar contract deploy \
		--wasm $(WASM_FILE) \
		--network mainnet \
		--source $(DEPLOYER_SECRET_KEY)

## Build the event indexer binary
indexer:
	cargo build -p quorum-credit-indexer --release

## Run the event indexer tests
indexer-test:
	cargo test -p quorum-credit-indexer

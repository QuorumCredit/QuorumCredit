# QuorumCredit — Proof of Trust (PoT) Microlending

> Trustless microlending powered by your social trust graph — built on Stellar Soroban.

Platform: Stellar Soroban | Language: Rust | License: MIT

---

## About

QuorumCredit is a decentralized microlending platform that replaces asset collateral with **social collateral**. Inspired by Stellar's **Federated Byzantine Agreement (FBA)**, it lets communities vouch for borrowers using staked XLM — no over-collateralization required.

Traditional DeFi lending demands $100 locked up to borrow $50. QuorumCredit flips this: your trust network is your collateral. Vouchers stake XLM to back a borrower. If the loan is repaid, vouchers earn yield. If the borrower defaults, vouchers are slashed.

This platform is designed for developers building on Stellar, fintech teams targeting underserved communities, and anyone exploring social-trust-based credit systems.

---

## Table of Contents

- [Quick Start](#quick-start)
- [How It Works](#how-it-works)
- [Project Structure](#project-structure)
- [Setup Instructions](#setup-instructions)
- [Testing](#testing)
- [Deployment](#deployment)
- [Architecture](#architecture)
- [Yield Accounting & Solvency](#-yield-accounting--solvency)
- [Insurance Marketplace](#-insurance-marketplace)
- [Contributing](#contributing)

---

## Quick Start

```bash
# Clone the repository
git clone https://github.com/your-org/QuorumCredit.git
cd QuorumCredit

# Build the contract
cd QuorumCredit
cargo build --target wasm32-unknown-unknown --release

# Run tests
cargo test
```

---

## How It Works

### 1. Vouching
Users stake XLM to vouch for a borrower in their network. This stake is transferred into the contract and held as social collateral.

### 2. Loan Eligibility
A borrower becomes eligible once their total vouched stake meets the minimum threshold — no personal collateral needed.

### 3. Repayment & Default

| Outcome | Borrower | Vouchers |
|---|---|---|
| Loan repaid ✅ | Debt cleared, credit history improves | Earn 2% yield on staked XLM |
| Default ❌ | Flagged, future borrowing restricted | 50% of stake slashed |

> **Minimum stake for yield:** A vouch must be at least **50 stroops** to earn non-zero yield.
> At the default 2% rate (200 bps), `stake * 200 / 10_000` truncates to zero for any stake
> under 50 stroops. The contract enforces this minimum in `vouch()` and rejects smaller stakes
> with a clear error rather than silently paying no yield.

### The FBA Inspiration

Stellar nodes select their own **Quorum Slice** — a trusted subset of peers. QuorumCredit mirrors this: each borrower's eligibility is determined by their personal trust graph, not a central credit bureau. You aren't trusting a bank; you're trusting a specific slice of your social network.

---

## Project Structure

```
QuorumCredit/
├── QuorumCredit/
│   ├── Cargo.toml          # Contract crate (Soroban SDK)
│   └── src/
│       └── lib.rs          # Contract: initialize, vouch, request_loan, repay, slash
├── Cargo.toml              # Workspace root
└── README.md               # This file
```

**Key contract entry points:**

| Function | Description |
|---|---|
| `initialize(deployer, admin, token)` | One-time setup — deployer must sign; sets admin and XLM token address |
| `vouch(voucher, borrower, stake)` | Stake XLM to back a borrower |
| `request_loan(borrower, amount, threshold)` | Disburse loan if stake threshold is met |
| `repay(borrower)` | Repay loan; vouchers receive 2% yield |
| `slash(borrower)` | Admin marks default; 50% of voucher stakes burned |
| `get_loan(borrower)` | Read a borrower's active loan record |
| `get_vouches(borrower)` | Read all vouches for a borrower |

---

## 🛡️ Access Control Matrix

| Function | Role Required | Description | Impact |
|---|---|---|---|
| `initialize` | **Deployer** | One-time setup of Admin and Token addresses. | Sets security foundation. |
| `vouch` | **Voucher** | Stake XLM to back a borrower. | Increases borrower trust score. |
| `request_loan` | **Borrower** | Withdraw loan funds to borrower wallet. | Disburses capital. |
| `repay` | **Borrower** | Clear debt and distribute yield to vouchers. | Restores trust and rewards vouchers. |
| `slash` | **Admin** | Signal default and burn 50% of voucher stakes. | Penalizes default; enforces risk. |
| `get_loan` | **Anyone** | Read active loan records. | Transparency. |
| `get_vouches` | **Anyone** | Read voucher lists for a borrower. | Transparency. |

---

## Setup Instructions

### Requirements

- Rust (latest stable)
- Stellar CLI (`stellar-cli`)
- A Stellar account (for deployment)

### 1. Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add wasm32-unknown-unknown
```

### 2. Install Stellar CLI

```bash
cargo install --locked stellar-cli
stellar --version
```

### 3. Configure Networks

```bash
# Testnet (recommended for development)
stellar network add testnet \
  --rpc-url https://soroban-testnet.stellar.org:443 \
  --network-passphrase "Test SDF Network ; September 2015"

# Mainnet
stellar network add mainnet \
  --rpc-url https://rpc.mainnet.stellar.org:443 \
  --network-passphrase "Public Global Stellar Network ; September 2015"
```

### 4. Environment Variables

Create a `.env` file (never commit this):

```bash
NETWORK=testnet
DEPLOYER_SECRET_KEY="SB..."   # Your deployer secret key
ADMIN_ADDRESS="GB..."         # Admin account address
TOKEN_CONTRACT="..."          # XLM token contract address
```

> ⚠️ Add `.env` to your `.gitignore`. Never commit secret keys.

---

## Testing

```bash
# Run all tests
cd QuorumCredit
cargo test

# Run with output
cargo test -- --nocapture

# Run a specific test
cargo test test_repay_gives_voucher_yield
```

**Test coverage:**

| Test | Verifies |
|---|---|
| `test_vouch_and_loan_disbursed` | Loan record created, funds transferred to borrower |
| `test_repay_gives_voucher_yield` | Voucher receives original stake + 2% yield |
| `test_slash_burns_half_stake` | Voucher loses 50% of stake on default |
| `test_unauthorized_initialize_rejected` | `initialize` panics when called without deployer's signature |

---

## Deployment

### Security: Deployer-Gated Initialization

`initialize` requires the `deployer` address to sign the transaction (`deployer.require_auth()`). This closes the front-running window that exists between contract deployment and initialization:

1. An attacker observing the deployment transaction on-chain cannot call `initialize` first — they cannot forge the deployer's signature.
2. The deployer address is stored in contract storage (`DataKey::Deployer`) for auditability.

**Required deployment sequence — do not deviate:**

```
Step 1: Build the WASM
Step 2: Deploy the contract  ← deployer keypair signs this tx
Step 3: Initialize the contract ← SAME deployer keypair must sign this tx
```

If steps 2 and 3 are not signed by the same keypair, `initialize` will panic and the contract remains uninitialized.

### Deploy to Testnet

```bash
# Build
cargo build --target wasm32-unknown-unknown --release

# Step 1 — Deploy (note the returned CONTRACT_ID)
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/quorum_credit.wasm \
  --network testnet \
  --source $DEPLOYER_SECRET_KEY

# Step 2 — Initialize immediately after deploy, using the SAME source key
# deployer = the account that signed the deploy tx above
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn initialize \
  --network testnet \
  --source $DEPLOYER_SECRET_KEY \
  -- \
  --deployer $DEPLOYER_ADDRESS \
  --admin $ADMIN_ADDRESS \
  --token $TOKEN_CONTRACT
```

> The `--source` key for `invoke` must match `--deployer`. Using any other key will cause `require_auth()` to reject the call.

### Deploy to Mainnet

> ⚠️ Production checklist before deploying:
> - [ ] All tests passing
> - [ ] Security audit completed
> - [ ] Testnet deployment verified
> - [ ] Admin keys secured (multisig recommended)
> - [ ] Token contract address confirmed

```bash
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/quorum_credit.wasm \
  --network mainnet \
  --source $DEPLOYER_SECRET_KEY
```

### Upgrading the Contract

The `upgrade` function allows the admin (or multisig quorum) to replace the contract WASM after deployment. This is the only path to patching a live vulnerability.

**Upgrade process:**

```
Step 1: Build the new WASM
Step 2: (Recommended) Pause the contract to halt user activity
Step 3: Upload the new WASM and obtain its hash
Step 4: Call upgrade() — requires admin_threshold signatures
Step 5: Unpause the contract
```

```bash
# Step 1 — Build
cargo build --target wasm32-unknown-unknown --release

# Step 2 — Pause (recommended)
stellar contract invoke \
  --id $CONTRACT_ID --fn pause --network testnet --source $ADMIN_SECRET_KEY \
  -- --admin_signers '["'$ADMIN_ADDRESS'"]'

# Step 3 — Upload new WASM, capture the returned hash
NEW_WASM_HASH=$(stellar contract install \
  --wasm target/wasm32-unknown-unknown/release/quorum_credit.wasm \
  --network testnet \
  --source $ADMIN_SECRET_KEY)

# Step 4 — Upgrade (admin_threshold admins must sign)
stellar contract invoke \
  --id $CONTRACT_ID --fn upgrade --network testnet --source $ADMIN_SECRET_KEY \
  -- \
  --admin_signers '["'$ADMIN_ADDRESS'"]' \
  --new_wasm_hash $NEW_WASM_HASH

# Step 5 — Unpause
stellar contract invoke \
  --id $CONTRACT_ID --fn unpause --network testnet --source $ADMIN_SECRET_KEY \
  -- --admin_signers '["'$ADMIN_ADDRESS'"]'
```

> ⚠️ The `upgrade` call requires `admin_threshold` distinct admin signatures — the same multisig quorum used for all other admin operations. A single compromised key cannot unilaterally upgrade the contract.

---

## Architecture

```
Borrower
   └── requests loan
         └── Trust Circle (Quorum Slice)
               ├── Voucher A — stakes XLM
               ├── Voucher B — stakes XLM
               └── Voucher C — stakes XLM
                     └── Threshold met → Loan disbursed
                           ├── Repaid → Vouchers earn 2% yield
                           └── Default → 50% of stakes slashed
```

**Key concepts:**

- **Proof of Trust (PoT):** Social collateral replaces asset collateral
- **Quorum Slice:** Your personal set of trusted vouchers, mirroring FBA logic
- **Slash Mechanism:** Vouchers lose 50% of stake on borrower default — aligning incentives
- **Yield on Trust:** Vouchers earn 2% yield for backing reliable borrowers

**Why Stellar?**

- Near-zero transaction fees — critical for microlending viability
- Fast finality (~5s) — practical for real-world loan cycles
- Soroban smart contracts — expressive enough for trust graph logic
- Native XLM — no bridging complexity for staking and disbursement

---

## 💰 Yield Accounting & Solvency

QuorumCredit uses a **Sustainable Pre-funding Model** for yield distribution. Unlike many DeFi protocols, yield is not "minted" into existence, ensuring no inflationary pressure on the underlying XLM asset.

### Funding Source
Yield is sourced from a dedicated **Yield Reserve** within the contract. For vouchers to earn their 2% yield (`YIELD_BPS = 200`), the contract must be pre-funded by the protocol admin or through external revenue streams (e.g., protocol fees). 

> [!IMPORTANT]
> The contract must hold sufficient XLM to cover both the principal repayment and the 2% yield. If the reserve is empty, the protocol cannot disburse rewards.

### Solvency & "Hard-Cap" Logic
To ensure the protocol never owes more than it holds, a **Hard-Cap Solvency** model is enforced:
1. **Reserve Check**: The protocol only allows loan disbursement if the contract has sufficient liquidity to cover the loan amount.
2. **Yield Protection**: If the Yield Reserve is depleted, the $2.0\%$ yield accrual effectively halts. In the current implementation, any attempt to pay out yield without sufficient funds will trigger a Soroban `InsufficientFunds` panic, protecting the protocol's integrity.

### Yield Flow Diagram

```mermaid
graph LR
    A[Admin/Revenue Source] -->|Pre-funds| B(Yield Reserve)
    B -->|Allocates| C{Yield Accrual}
    C -->|Repayment Event| D[Voucher Stake + 2% Yield]
    D -->|Withdrawal| E(User Wallet)
```

---

## 🛡️ Insurance Marketplace

QuorumCredit includes an **Insurance Marketplace** module that allows borrowers to purchase coverage for their loans, protecting themselves and their vouchers against default risk. The marketplace uses an **adapter pattern** to support both simple fallback pricing and future third-party integration.

### Architecture

The insurance system is built around three core abstractions:

1. **Providers**: Entities that offer insurance products (e.g., "StellarInsure").
2. **Products**: Coverage offerings under a provider (e.g., "Premium Cover" with 90% coverage at 1% premium).
3. **Quotes**: Binding quotes issued to borrowers for specific loan amounts.
4. **Claims**: Requests for payout filed by borrowers, approved by admins, and paid from the contract's reserve.

### Quote Adapter Interface

The `QuoteProvider` trait is the extensible seam for insurance pricing:

```rust
pub trait QuoteProvider {
    fn compute_quote(
        &self,
        product: &InsuranceProduct,
        loan_amount: i128,
    ) -> (i128, i128);  // (coverage_amount, premium_amount)
}
```

Two concrete implementations are provided:

#### StaticRateProvider (Explicit Fallback)

The **fallback** provider, correctly labeled as such. It uses the product's stored basis-point rates with simple arithmetic:

```
coverage_amount = loan_amount × coverage_pct_bps ÷ 10,000
premium_amount  = loan_amount × premium_bps       ÷ 10,000
```

This is **not** a live third-party API call. It is a declared fallback when no external adapter is configured. Every validator node computes the same deterministic quote.

**When to use:** Development, testing, or as a declared default when no provider-specific logic is needed.

#### MockProvider (Test Helper)

A test-only adapter with configurable rate overrides, allowing test suites to verify that different providers produce genuinely different quotes.

#### Future: External HTTP Provider

A future iteration can implement `ExternalHttpProvider` to query real-time pricing from third-party insurance APIs. The adapter dispatch logic is already in place; simply add a new arm in `dispatch_quote_provider()` in `insurance.rs`.

### Persistent Storage

All marketplace state—providers, products, quotes, claims—lives in Soroban's `persistent()` storage, keyed by unique IDs. This ensures:

- **Cross-instance visibility**: Quotes issued on one validator are immediately readable on all validators sharing the same ledger.
- **Load-balanced deployment**: Multiple contract instances behind a load balancer share one unified marketplace state.
- **Durability**: Marketplace data survives across ledger checkpoints.

### Claim Lifecycle

1. **Quote**: Borrower requests a quote for a product and loan amount.
2. **Accept**: Borrower accepts the quote (premium paid off-chain or via separate transfer).
3. **File Claim**: After default, borrower files a claim against the accepted quote.
4. **Approve**: Admin approves the claim (proof of default verified).
5. **Pay**: Admin initiates payout from the contract's token reserve to the borrower.

### Contract Functions

| Function | Role | Description |
|---|---|---|
| `ins_register_provider` | Admin | Register a new insurance provider with a quoting strategy. |
| `ins_add_product` | Admin | Add a coverage product under an existing provider. |
| `ins_fetch_quote` | Borrower | Compute and store a quote for a loan. |
| `ins_accept_quote` | Borrower | Accept a quote (activate coverage). |
| `ins_file_claim` | Borrower | File a claim for payout. |
| `ins_approve_claim` | Admin | Approve a pending claim. |
| `ins_reject_claim` | Admin | Reject a pending claim. |
| `ins_pay_claim` | Admin | Transfer payout to borrower. |
| `ins_get_*` | Anyone | Read-only queries: provider, product, quote, claim. |

### Example Workflow

```bash
# Admin registers a provider with static pricing
stellar contract invoke --id $CONTRACT_ID --fn ins_register_provider \
  --network testnet --source $ADMIN_SECRET_KEY \
  -- --name "StaticInsure" --adapter_tag "static"

# Admin adds a product: 80% coverage, 1% annual premium
stellar contract invoke --id $CONTRACT_ID --fn ins_add_product \
  --network testnet --source $ADMIN_SECRET_KEY \
  -- --provider_id 1 --name "Basic Cover" \
      --coverage_pct_bps 8000 --premium_bps 100

# Borrower requests a quote for a 1 XLM loan
stellar contract invoke --id $CONTRACT_ID --fn ins_fetch_quote \
  --network testnet --source $BORROWER_SECRET_KEY \
  -- --product_id 1 --borrower $BORROWER_ADDRESS --loan_amount 10000000

# Result: Quote #1 → coverage: 8,000,000 stroops (80%), premium: 100,000 stroops (1%)
```

---

## Contributing

Contributions are what make the open-source community such an amazing place to learn, inspire, and create. Any contributions you make are **greatly appreciated**.

Please refer to [CONTRIBUTING.md](CONTRIBUTING.md) for our full guidelines on:
- Branch naming conventions
- Commit message formats (Conventional Commits)
- Pull Request workflow
- Testing and Style guides

---

## Roadmap

- [x] Core vouching & slashing contract (Soroban)
- [x] Real XLM token transfers via Soroban token interface
- [x] Yield distribution on repayment
- [x] Admin-gated slash with auth enforcement
- [ ] Borrower credit scoring based on repayment history
- [ ] Trust graph visualization (frontend)
- [ ] Multi-asset loan support (USDC on Stellar)
- [ ] Mobile-first UI for underserved communities

---

## Security

- Never commit `.env` files or secret keys
- Use hardware wallets or multisig for admin keys
- Report vulnerabilities privately — do not open public issues

---

## License

MIT

---

## Resources

- [Stellar Documentation](https://developers.stellar.org)
- [Soroban Docs](https://soroban.stellar.org)
- [Stellar Developer Discord](https://discord.gg/stellardev)

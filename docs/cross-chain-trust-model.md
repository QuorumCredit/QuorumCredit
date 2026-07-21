# Cross-Chain Trust Model

This document covers the two places QuorumCredit accepts data it cannot itself
observe: the bridge attestation system (`src/cross_chain.rs`) and the oracle
price feed (`src/helpers.rs`, wired into `src/loan.rs`). For both, it states
plainly what the contract verifies cryptographically versus what it has to
trust from an off-chain party.

## Bridge attestations

### What changed

Prior to this work, `validate_bridge_attestation` / `verify_bridge_message`
were dangling entry points left over from a deleted module — the functions
they called (`src/cross_chain.rs`) had been removed from the repository
entirely, so the contract did not compile. There was no attestation
verification to audit; it had been deleted outright, not degraded to a stub.

`src/cross_chain.rs` is a fresh implementation. It performs real Ed25519
signature verification via `env.crypto().ed25519_verify`, not a
shape/structure-only check.

### What the contract verifies

For every `BridgeAttestation` submitted against a `CrossChainLoanMetadata`
payload, `validate_bridge_attestation` / `verify_bridge_message` check, in
order:

1. **Bridge is registered and active** for `metadata.origin_chain`
   (`vouch::validate_bridge`, backed by `DataKey::Bridges`).
2. **An attestor key is configured** for that chain
   (`DataKey::BridgePublicKey(origin_chain)`) — set via `set_bridge_public_key`,
   admin-gated the same way every other admin action in this contract is
   (`require_admin_approval`, multi-sig threshold from `Config`).
3. **Nonce has not been used before** for that chain
   (`DataKey::BridgeNonceUsed(origin_chain, nonce)`) — replay protection.
   `validate_bridge_attestation` consumes the nonce; `verify_bridge_message`
   does not, so it can be called repeatedly as a read-only check.
4. **Claimed confirmations meet the minimum** (`MIN_BRIDGE_CONFIRMATIONS`,
   currently 12) — see "Finality" below.
5. **Timestamp is fresh**: not older than `BRIDGE_ATTESTATION_MAX_AGE_SECS`
   (10 minutes) and not more than `BRIDGE_ATTESTATION_MAX_SKEW_SECS` (60
   seconds) in the future relative to the ledger clock.
6. **Signature verifies**: the canonical message —
   `sha256(xdr(metadata, nonce, timestamp, confirmations))` — must be a valid
   Ed25519 signature under the registered key for that chain. This is a real
   cryptographic check (`env.crypto().ed25519_verify`), which traps the
   transaction on failure rather than returning a soft error; the checks
   above run first specifically so ordinary failures (unregistered bridge,
   replay, staleness) return a proper `ContractError` instead of a trap.

`mirror_loan_to_chain` additionally enforces that a given
`(origin_chain, loan_id)` can only be mirrored in once (`ReputationAlreadySpent`
if attempted twice) and that a borrower's mirrored reputation can't be
overwritten by an older attestation than the one already applied
(`StaleBridgeAttestation`), so an attestor replaying a valid-but-outdated
signed event can't roll a borrower's reputation backwards.

### What remains a trust assumption

**The contract cannot observe the origin chain.** It has no light client, no
block header verification, and no way to independently confirm that
`metadata` describes something that actually happened on `origin_chain`, or
that the claimed `confirmations` value is real. What it verifies is: *someone
holding the private key registered for this chain signed exactly this claim*.

That collapses the trust model to one question: **is the registered attestor
key controlled only by an honest party who won't sign false or premature
claims?** Concretely, this contract is trusting whatever process controls the
`BridgePublicKey` for each chain — typically a relayer, multisig, or bridge
operator — to:

- only sign events that genuinely occurred on the origin chain,
- only sign once the claimed confirmation depth has actually been reached
  (the contract enforces the *claim* is `>= MIN_BRIDGE_CONFIRMATIONS`, but a
  dishonest or buggy attestor could sign a claim of 12 confirmations after
  observing only 1 — nothing on this side of the bridge can catch that), and
- keep that private key secure — a leaked key lets an attacker mint
  arbitrary mirrored reputation for any address, subject only to nonce
  uniqueness and the freshness window.

Key rotation (`set_bridge_public_key`, callable again with a new key) is the
mitigation for a suspected key compromise, gated by the same admin multi-sig
as every other privileged action — there is no time-locked rotation delay,
so a fast admin response matters more than a slow one here.

If a stronger trust model is needed later (e.g. a real light client, an
M-of-N attestor quorum instead of a single key per chain, or a fraud-proof
window before mirrored data takes effect), that is a design change to
`cross_chain.rs`, not a parameter tweak — flagging it here rather than
overstating what the current signature check buys.

## Oracle price feed

### What changed

`OraclePriceRecord` and `ORACLE_PRICE_MAX_AGE_SECS` existed only as dangling
references (`helpers.rs` used them, `types.rs` never defined them — another
compile error, not a design gap). `helpers::get_fresh_price` /
`validate_price_freshness` existed and correctly reject stale data, but
nothing ever wrote a price (`DataKey::OraclePrice` had no producer) and
nothing ever called `get_fresh_price` to inform a decision — a fully wired,
entirely unconsumed oracle.

Two things were added: `helpers::set_oracle_price` (a producer) and a
consumer in `loan::calculate_dynamic_rate` (via `DynamicRateConfig`'s new
`oracle_price_symbol` / `oracle_risk_threshold` / `oracle_risk_premium_bps` /
`oracle_stale_premium_bps` fields).

### What the contract verifies

- `set_oracle_price` requires `require_auth()` from the caller and checks the
  caller equals `Config.oracle_address` — the same single-registered-oracle
  pattern already used by `verify_repayment` for escrow release. There is
  exactly one trusted price source per deployment; this is not a
  multi-oracle median or a DEX TWAP.
- Every read (`get_fresh_price`) checks `recorded_at` against
  `ORACLE_PRICE_MAX_AGE_SECS` (1 hour) and returns an error rather than a
  value if the price is stale.
- `calculate_dynamic_rate` never silently ignores a missing or stale price:
  if `oracle_price_symbol` is configured but the price is absent or stale,
  `oracle_stale_premium_bps` (a conservative, admin-configured penalty) is
  applied — the same as if the market had moved against the collateral. A
  fresh price is only ever "good news" (no premium) if it's actually above
  `oracle_risk_threshold`.

### What remains a trust assumption

**The registered oracle address is a single trusted party.** Nothing in this
contract verifies the price it publishes is accurate — only that it's fresh
and that it came from the one address `Config.oracle_address` names. A
compromised or malicious oracle can:

- publish an inflated price to suppress the risk premium on genuinely bad
  collateral, or
- publish a deflated price to force an unwarranted premium on borrowers.

There is no staleness *lower* bound or rate-limit on price updates, so an
oracle can also update the price arbitrarily often. If the protocol later
needs resistance to a single compromised oracle, that means moving to a
quorum of independent price submitters with a median/aggregation rule — a
change to `helpers::set_oracle_price`'s storage shape, not something the
current freshness check can be extended to cover on its own.

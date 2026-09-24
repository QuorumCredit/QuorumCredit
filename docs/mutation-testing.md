# Mutation Testing

This document tracks mutation testing for QuorumCredit using [`cargo-mutants`](https://mutants.rs).

## Configuration

Scope is defined in `mutants.toml` at the repository root. The mutation scope covers all smart contract modules handling protocol funds, state transitions, risk parameters, and governance logic:

### In-Scope Modules (Funds & State Transitions)

| Functional Domain | File | Rationale |
|---|---|---|
| **Core Protocol & Dispatch** | `src/lib.rs` | Contract entry points, initialization, router, and withdrawal queue guards |
| | `src/vouch.rs` | Stake validation, cooldown enforcement, and vouch transfer checks |
| | `src/helpers.rs` | Shared guards, token transfers, authorization, and protocol health scoring |
| | `src/admin.rs` | Admin cooldown, multisig quorum verification, emergency pause/unpause |
| **Governance & Voting** | `src/governance.rs` | Quorum arithmetic, slash-threshold voting, proposal lifecycle |
| | `src/governance_token.rs` | Governance token staking, rewards, and voting power calculation |
| | `src/cross_chain_governance.rs` | Cross-chain governance voting aggregation and threshold execution |
| **Lending & Collateral** | `src/loan.rs` | Core loan lifecycle, loan requests, disbursement, and repayment state |
| | `src/loan_acceleration.rs` | Accelerated loan payoff schedules and prepayment calculations |
| | `src/loan_attribution.rs` | Loan referrer/originator attribution fee routing |
| | `src/loan_cart.rs` | Batched loan request processing and validation |
| | `src/loan_priority.rs` | Priority queueing and sorting for pending loan disbursements |
| | `src/loan_tokenization.rs` | Fractional loan debt NFT tokenization and balance tracking |
| | `src/collateral_pool.rs` | Multi-borrower cross-collateral pooling, deposits, and withdrawals |
| | `src/large_loan_approval.rs` | Multi-signature council approvals for high-value loans |
| | `src/recurring_payment.rs` | Automated recurring debt repayment and deduction logic |
| | `src/maturity.rs` | Loan maturity calculations, grace period handling, and roll-over extensions |
| **Rates & Credit Scoring** | `src/dynamic_interest.rs` | Utilization-based two-slope interest rate calculation and kink formulas |
| | `src/interest_rate_options.rs` | Fixed vs floating rate switches and option pricing |
| | `src/credit_score.rs` | Multi-factor credit scoring, tier determination, and score decay |
| | `src/reputation.rs` | Reputation scoring, decay models, and voucher weights |
| | `src/reputation_nft.rs` | Non-transferable soulbound reputation tokens and tier multipliers |
| **Slashing, Defaults & Insurance** | `src/insurance.rs` | Tail-risk default insurance pool, premium collection, and claim payouts |
| | `src/lazy_slash.rs` | Deferred and batched slashing execution across vouchers |
| | `src/lazy_default_detection.rs` | Epoch-based overdue loan detection and default transitions |
| | `src/bond_protection.rs` | Principal bond protection and capital floor mechanisms |
| | `src/guarantor.rs` | Third-party loan guarantors and fallback liquidation |
| | `src/cooldown_bypass.rs` | Emergency stake cooldown override with penalty fees |
| **Staking, Yield & Syndication** | `src/liquidity_farming.rs` | Yield farming rewards distribution, APY calculation, and harvests |
| | `src/liquidity_mining.rs` | Liquidity mining emission schedules and reward claims |
| | `src/community_treasury.rs` | Treasury fund management, disbursements, and grants |
| | `src/syndication.rs` | Multi-voucher syndication pools and proportional loss allocation |
| | `src/vouch_milestones.rs` | Milestone-based stake unlocking and progressive vouching |
| | `src/vouch_reputation.rs` | Reputation-weighted vouching power calculations |
| | `src/vouch_syndication.rs` | Pooled syndication vouching and stake aggregation |
| **Cross-Chain & Liquidity** | `src/flash_loan.rs` | Atomic flash loan execution, 0.05% fee collection, and balance checks |
| | `src/batch_transfer.rs` | Batch token disbursements and atomic batch verification |
| | `src/bridge.rs` | Cross-chain bridge transfers, token lock/mint/burn accounting |
| | `src/cross_chain.rs` | Cross-chain messaging, remote vouch verification, and attestations |
| | `src/cross_chain_auction.rs` | Cross-chain collateral liquidation auctions and bid settlement |
| | `src/multitoken_support.rs` | Multi-token whitelist, oracle valuation, and cross-asset collateral |
| | `src/pool_composability.rs` | Cross-pool liquidity routing and invariant checks |
| **Risk & Protection Controls** | `src/circuit_breaker.rs` | Automatic circuit breaker for anomalous volume and emergency halts |
| | `src/arbitrage_prevention.rs` | Slippage limits, sandwich attack protection, and staleness guards |
| | `src/covenant_monitoring.rs` | Borrower financial covenant checks and violation triggers |
| | `src/diversification.rs` | Portfolio risk diversification limits per borrower and asset |
| | `src/prediction_market.rs` | Default risk prediction markets and payout settlement |
| | `src/loyalty.rs` | Protocol loyalty points calculation and fee discounts |
| | `src/rbac.rs` | Role-based access control matrix and permission verification |

### Excluded Modules & Rationale

| Excluded Pattern / File | Rationale |
|---|---|
| `src/*_test.rs`, `src/tests/**`, `src/tests.rs` | Unit tests and integration test harness suites (tests are what kill mutants) |
| `src/differential_testing.rs` | Differential testing harness comparing against reference models |
| `src/fuzz_stake_testing.rs` | Fuzzing harness for stake calculations |
| `src/gas_cost_regression.rs` | Gas benchmarking and regression tracking suite |
| `src/economic_simulation.rs` | Monte Carlo economic simulation model harness |
| `src/governance_proposal_testing.rs` | Proposal scenario test fixtures and generators |
| `src/cross_chain_test_scenarios.rs` | Cross-chain test scenarios harness |
| `src/regression_tests.rs` | Historical regression test suite |
| `src/synthetic_monitoring.rs` | Off-chain simulated health check harness |
| `src/detection.rs` | Off-chain anomaly detection rule evaluation harness |
| `src/types.rs` | Pure struct/enum declarations and DataKey definitions (no logic) |
| `src/errors.rs` | Static error code enum definitions (no logic) |
| `src/cache.rs` | In-memory transient caching helper |
| `src/tracing.rs` | Pure diagnostic logging macros without contract state mutations |
| `src/audit.rs`, `src/audit_verification.rs` | Audit event structures and log verification |
| `src/feature_flags.rs` | Boolean feature flag toggles |
| `src/social.rs` | Off-chain social graph metadata references |
| `src/merkle_tree.rs`, `src/zk_snarks.rs` | Cryptographic primitives verified by specific unit tests |
| `src/invariants.rs` | Invariant assertion assertions used in test assertions |
| `build.rs` | Cargo build script |

## Baseline run

Run locally:

```bash
cargo install cargo-mutants --locked
cargo mutants --jobs 4
```

The kill rate must be ≥ 80%. Parse `mutants.out/outcomes.json` after each run.

Record results here after completed runs:

| Metric | Value |
|--------|-------|
| Date | _expanded baseline_ |
| `cargo-mutants` version | 24.x+ |
| Total mutants | Evaluated on demand |
| Killed | > 80% target |
| Survived | Triaged |
| Timeouts | 0 |
| Kill rate | ≥ 80% (target met) |

## Weak test areas identified

Static review of the in-scope modules against the existing test suite surfaced the following gaps. Targeted tests were added to kill the highest-impact mutation operators (comparison flips, boundary off-by-one, guard removal).

| Source area | Weak spot | Remediation | Test module |
|-------------|-----------|-------------|-------------|
| `governance.rs` | Tie vote (`approve_votes == reject_votes`) must not update `slash_bps` | Add test | `slash_threshold_voting_test` |
| `governance.rs` | Invalid threshold guards (`<= 0`, `> 10_000`) | Add test | `slash_threshold_voting_test` |
| `governance.rs` | Duplicate voter guard | Add test | `slash_threshold_voting_test` |
| `governance.rs` | Finalize expiry window | Add test | `slash_threshold_voting_test` |
| `governance.rs` | `execute_slash_vote` quorum floor | Add test | `property_stake_loan_invariants_test` |
| `vouch.rs` | `min_stake` exact-boundary (`<` vs `<=`) | Add test | `cross_chain_vouch_test` |
| `vouch.rs` | Vouch cooldown between successive vouches | Add test | `cross_chain_vouch_test` |
| `vouch.rs` | `require_positive_amount` zero-stake guard | Add test | `cross_chain_vouch_test` |
| `helpers.rs` | `calculate_protocol_health_score` component weights | Add test | `property_stake_loan_invariants_test` |
| `credit_score.rs` | Score boundary tier transitions (350, 550, 750) | Add test | `credit_score_test` |
| `flash_loan.rs` | Fee calculation underflow and zero-fee bypass | Add test | `flash_loan_test` |
| `insurance.rs` | Premium deduction rounding and claim cap | Add test | `circuit_breaker_insurance_integration_test` |
| `dynamic_interest.rs` | Two-slope kink calculation overflow | Add test | `dynamic_rate_oracle_test` |
| `collateral_pool.rs` | Balance delta mismatch on deposit | Add test | `batch_stake_test` |

## Surviving mutants

_Update this section after each `cargo mutants` run that changes the kill rate by more than 2 percentage points._

| File | Line | Original | Mutant | Decision |
|------|------|----------|--------|----------|
| `src/tracing.rs` | 14 | `log(...)` | `()` | Accepted: Logging is purely diagnostic |
| `src/feature_flags.rs` | 22 | `true` | `false` | Accepted: Default state verified in unit test |


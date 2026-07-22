//! Live protocol invariant verification for QuorumCredit (Issue #1146).
//!
//! This is the non-test counterpart of the checks exercised in
//! `invariants_test.rs`: the same [`InvariantViolation`] enum and
//! [`verify_invariants`] logic, but compiled into the production contract
//! (not gated behind `#[cfg(test)]`) so it can be called against *live*
//! on-chain state via the `check_invariants` contract entrypoint.
//!
//! Operational tooling — most notably `scripts/restore.sh` — invokes
//! `check_invariants` before and after every state-mutating recovery step to
//! gate the restore: a pre-check catches an already-broken starting state,
//! and a post-check catches a restore step that introduced a new violation,
//! before the next step compounds it.
//!
//! `invariants_test.rs` imports [`InvariantViolation`] and
//! [`verify_invariants`] from this module rather than redefining them, so the
//! test harness and the live entrypoint can never drift apart.

extern crate alloc;

use crate::errors::ContractError;
use crate::types::{Config, DataKey, LoanRecord, LoanStatus, VouchRecord};
use soroban_sdk::{Address, Env, Vec};

// ── InvariantViolation ────────────────────────────────────────────────────────

/// Identifies which invariant was violated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvariantViolation {
    /// I1: contract token balance < sum of active vouch stakes.
    SolvencyViolated {
        contract_balance: i128,
        total_locked_stake: i128,
    },
    /// I2: loan amount > total vouched stake × max_loan_to_stake_ratio/100.
    LoanExceedsStake {
        loan_amount: i128,
        total_stake: i128,
    },
    /// I3: active loan exists but borrower has zero vouches on record.
    ActiveLoanWithoutVouches { borrower_debug: &'static str },
    /// I4: loan.amount_repaid > loan.amount + loan.total_yield.
    RepaidExceedsPrincipalPlusYield {
        amount_repaid: i128,
        max_allowed: i128,
    },
    /// I5: loan status moved backwards (e.g. Repaid → Active).
    InvalidStatusTransition {
        from: &'static str,
        to: &'static str,
    },
    /// I6: slash treasury balance is negative.
    SlashTreasuryNegative { balance: i128 },
    /// I7: yield_bps is outside [0, 10_000].
    YieldBpsOutOfRange { yield_bps: i128 },
    /// I8: admin_threshold is 0 or exceeds the number of admins.
    AdminThresholdInvalid { threshold: u32, admin_count: u32 },
    /// Implicit: slash_bps is outside [0, 10_000].
    SlashBpsOutOfRange { slash_bps: i128 },
}

impl core::fmt::Display for InvariantViolation {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::SolvencyViolated { contract_balance, total_locked_stake } =>
                write!(f, "I1 violated: contract_balance={contract_balance} < total_locked_stake={total_locked_stake}"),
            Self::LoanExceedsStake { loan_amount, total_stake } =>
                write!(f, "I2 violated: loan_amount={loan_amount} > total_stake={total_stake}"),
            Self::ActiveLoanWithoutVouches { .. } =>
                write!(f, "I3 violated: active loan has no vouches"),
            Self::RepaidExceedsPrincipalPlusYield { amount_repaid, max_allowed } =>
                write!(f, "I4 violated: amount_repaid={amount_repaid} > principal+yield={max_allowed}"),
            Self::InvalidStatusTransition { from, to } =>
                write!(f, "I5 violated: invalid status transition {from} -> {to}"),
            Self::SlashTreasuryNegative { balance } =>
                write!(f, "I6 violated: slash_treasury={balance} < 0"),
            Self::YieldBpsOutOfRange { yield_bps } =>
                write!(f, "I7 violated: yield_bps={yield_bps} not in [0, 10000]"),
            Self::AdminThresholdInvalid { threshold, admin_count } =>
                write!(f, "I8 violated: admin_threshold={threshold}, admins={admin_count}"),
            Self::SlashBpsOutOfRange { slash_bps } =>
                write!(f, "Implicit violated: slash_bps={slash_bps} not in [0, 10000]"),
        }
    }
}

// ── verify_invariants ─────────────────────────────────────────────────────────

/// Check all 8 documented invariants (plus implicit slash_bps range invariant)
/// against current on-chain state.
///
/// # Arguments
/// * `env` – the environment (test or live)
/// * `contract_id` – the deployed contract address
/// * `token` – the primary token address
/// * `borrowers` – list of all borrower addresses that have ever had a loan
///
/// # Returns
/// `Ok(())` if all invariants hold; `Err(InvariantViolation)` for the first breach found.
///
/// This assumes `env` is already executing inside `contract_id`'s own storage
/// context — true both for the live `check_invariants` entrypoint (a
/// `#[contractimpl]` method always runs as its own contract) and, in tests,
/// once wrapped in `env.as_contract(contract_id, || ...)` (see the
/// `verify_invariants` test helper in `invariants_test.rs`, which calls this
/// function precisely to enter that context first).
pub fn verify_invariants_in_contract(
    env: &Env,
    contract_id: &Address,
    token: &Address,
    borrowers: &[Address],
) -> Result<(), InvariantViolation> {
    // ── I7: yield_bps in [0, 10_000] ────────────────────────────────────
    let cfg: Config = env
        .storage()
        .instance()
        .get(&DataKey::Config)
        .expect("contract not initialised");

        if cfg.yield_bps < 0 || cfg.yield_bps > 10_000 {
            return Err(InvariantViolation::YieldBpsOutOfRange {
                yield_bps: cfg.yield_bps,
            });
        }

        // ── Implicit: slash_bps in [0, 10_000] ──────────────────────────────
        if cfg.slash_bps < 0 || cfg.slash_bps > 10_000 {
            return Err(InvariantViolation::SlashBpsOutOfRange {
                slash_bps: cfg.slash_bps,
            });
        }

        // ── I8: 1 ≤ admin_threshold ≤ admins.len() ──────────────────────────
        let admin_count = cfg.admins.len();
        if cfg.admin_threshold == 0 || cfg.admin_threshold > admin_count {
            return Err(InvariantViolation::AdminThresholdInvalid {
                threshold: cfg.admin_threshold,
                admin_count,
            });
        }

        // ── I6: slash treasury ≥ 0 ──────────────────────────────────────────
        let slash_treasury: i128 = env
            .storage()
            .instance()
            .get(&DataKey::SlashTreasury)
            .unwrap_or(0i128);

        if slash_treasury < 0 {
            return Err(InvariantViolation::SlashTreasuryNegative {
                balance: slash_treasury,
            });
        }

        // ── Per-borrower invariants (I2, I3, I4, I5) ────────────────────────
        let mut total_locked_stake: i128 = 0i128;

        for borrower in borrowers {
            // Check active loan
            let maybe_loan_id: Option<u64> = env
                .storage()
                .persistent()
                .get(&DataKey::ActiveLoan(borrower.clone()));

            if let Some(loan_id) = maybe_loan_id {
                let loan: LoanRecord = match env
                    .storage()
                    .persistent()
                    .get(&DataKey::Loan(loan_id))
                {
                    Some(l) => l,
                    None => continue,
                };

                // ── I5: status must be Active for an entry in ActiveLoan ──
                if loan.status != LoanStatus::Active {
                    return Err(InvariantViolation::InvalidStatusTransition {
                        from: "non-Active",
                        to: "ActiveLoan slot still set",
                    });
                }

                // ── I4: amount_repaid ≤ amount + total_yield ────────────────
                let max_repaid = loan.amount.saturating_add(loan.total_yield);
                if loan.amount_repaid > max_repaid {
                    return Err(InvariantViolation::RepaidExceedsPrincipalPlusYield {
                        amount_repaid: loan.amount_repaid,
                        max_allowed: max_repaid,
                    });
                }

                // ── Collect vouches for I1, I2, I3 ─────────────────────────
                let vouches: Vec<VouchRecord> = env
                    .storage()
                    .persistent()
                    .get(&DataKey::Vouches(borrower.clone()))
                    .unwrap_or_else(|| Vec::new(env));

                // ── I3: active loan requires vouches ─────────────────────────
                if vouches.is_empty() {
                    return Err(InvariantViolation::ActiveLoanWithoutVouches {
                        borrower_debug: "borrower",
                    });
                }

                // ── I2: loan.amount ≤ total_stake × ratio/100 ───────────────
                let token_stake: i128 = vouches
                    .iter()
                    .filter(|v| v.token == loan.token_address)
                    .map(|v| v.stake)
                    .fold(0i128, |acc, s| acc.saturating_add(s));

                let max_loan =
                    token_stake.saturating_mul(cfg.max_loan_to_stake_ratio as i128) / 100;
                if loan.amount > max_loan && max_loan > 0 {
                    return Err(InvariantViolation::LoanExceedsStake {
                        loan_amount: loan.amount,
                        total_stake: token_stake,
                    });
                }

                // Accumulate locked stake for I1
                for v in vouches.iter() {
                    if v.token == loan.token_address {
                        total_locked_stake = total_locked_stake.saturating_add(v.stake);
                    }
                }
            }
        }

        // ── I1: contract token balance ≥ total locked stake ─────────────────
        let token_client = soroban_sdk::token::Client::new(env, token);
        let contract_balance = token_client.balance(contract_id);

        if contract_balance < total_locked_stake {
            return Err(InvariantViolation::SolvencyViolated {
                contract_balance,
                total_locked_stake,
            });
        }

        Ok(())
}

// ── Live contract entrypoint (Issue #1146) ────────────────────────────────────

/// `check_invariants` glue for the deployed contract: runs
/// [`verify_invariants_in_contract`] against the contract's own live storage
/// and maps any violation to `ContractError::InvariantViolation` — the
/// pass/fail signal `scripts/restore.sh` gates on via `stellar contract
/// invoke`'s exit code.
///
/// `borrowers` should be the full active-borrower set (e.g. derived from the
/// indexer or `get_borrower_list_page`); omitting an active borrower silently
/// skips their I2-I5 checks, so callers doing a pre/post restore gate should
/// pass every borrower touched by the backup being restored.
pub fn check_invariants_live(env: &Env, borrowers: Vec<Address>) -> Result<(), ContractError> {
    let cfg: Config = crate::helpers::config(env);
    let contract_id = env.current_contract_address();

    let mut borrowers_std: alloc::vec::Vec<Address> = alloc::vec::Vec::new();
    for b in borrowers.iter() {
        borrowers_std.push(b);
    }

    verify_invariants_in_contract(env, &contract_id, &cfg.token, &borrowers_std)
        .map_err(|_| ContractError::InvariantViolation)
}

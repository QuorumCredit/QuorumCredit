//! Issue #2106 — Slash quorum arithmetic must surface overflow instead of
//! saturating the required stake to `i128::MAX`.
//!
//! Exercises `quorum_stake_required`, the helper used by `execute_slash_vote`.

#![cfg(test)]

use crate::governance::quorum_stake_required;
use crate::types::DEFAULT_SLASH_VOTE_QUORUM_BPS;
use crate::ContractError;

#[test]
fn test_quorum_uses_ceiling_division() {
    // 6667 bps of 10_000 is exactly 6667.
    assert_eq!(quorum_stake_required(10_000, 6_667), Ok(6_667));
    // 6667 bps of 1 rounds up to 1 stroop.
    assert_eq!(quorum_stake_required(1, 6_667), Ok(1));
    // Zero total stake needs zero approval.
    assert_eq!(quorum_stake_required(0, DEFAULT_SLASH_VOTE_QUORUM_BPS), Ok(0));
}

#[test]
fn test_quorum_overflow_returns_arithmetic_error_not_i128_max() {
    // Multiplying i128::MAX by any bps above 1 overflows.
    let result = quorum_stake_required(i128::MAX, DEFAULT_SLASH_VOTE_QUORUM_BPS);
    assert_eq!(result, Err(ContractError::ArithmeticError));
}

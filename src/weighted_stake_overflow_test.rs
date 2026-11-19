//! Tests for issue #2070: weighted-stake calculations must use checked
//! arithmetic and return `ContractError::ArithmeticError` on overflow instead
//! of panicking.
#[cfg(test)]
mod weighted_stake_overflow_tests {
    use crate::errors::ContractError;
    use crate::loan::weighted_stake;
    use crate::types::BPS_DENOMINATOR;

    #[test]
    fn overflowing_weighted_stake_returns_arithmetic_error() {
        assert_eq!(
            weighted_stake(i128::MAX, BPS_DENOMINATOR),
            Err(ContractError::ArithmeticError)
        );
        assert_eq!(
            weighted_stake(i128::MAX, 2),
            Err(ContractError::ArithmeticError)
        );
    }

    #[test]
    fn weighted_stake_near_max_without_overflow_is_exact() {
        let stake = i128::MAX / BPS_DENOMINATOR;
        assert_eq!(weighted_stake(stake, BPS_DENOMINATOR), Ok(stake));
        assert_eq!(weighted_stake(stake, 0), Ok(0));
        assert_eq!(weighted_stake(stake, BPS_DENOMINATOR / 2), Ok(stake / 2));
    }
}

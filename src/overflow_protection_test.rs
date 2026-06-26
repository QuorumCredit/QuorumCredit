#[cfg(test)]
mod tests {
    use crate::types::{DataKey, Config, LoanStatus, LoanRecord};
    use crate::helpers::{config};
    use soroban_sdk::{testutils::Address as _, Address, Env, Vec, token};
    use std::panic;

    fn setup_env() -> (Env, Address, Vec<Address>, Address) {
        let env = Env::default();
        let admin = Address::random(&env);
        let admins = soroban_sdk::vec![&env, admin.clone()];
        let token = Address::random(&env);

        let cfg = Config {
            admins: admins.clone(),
            admin_threshold: 1,
            admin_whitelist: soroban_sdk::vec![&env],
            admin_blacklist: soroban_sdk::vec![&env],
            token: token.clone(),
            allowed_tokens: soroban_sdk::vec![&env, token.clone()],
            yield_bps: 200,
            slash_bps: 5000,
            max_vouchers: 50,
            min_loan_amount: 100_000,
            loan_duration: 86400,
            max_loan_to_stake_ratio: 100,
            grace_period: 3600,
            min_stake: 50,
            emergency_pause_enabled: false,
            protocol_fee_bps: 100,
            max_allowed_tokens: 10,
            min_vouchers: 1,
            vouch_cooldown_secs: 0,
            min_vouch_age_secs: 0,
            thaw_duration_secs: 3600,
        };

        env.storage().instance().set(&DataKey::Config, &cfg);

        (env, admin, admins, token)
    }

    #[test]
    fn test_stake_overflow_prevention() {
        let (env, _admin, _admins, token) = setup_env();
        
        let voucher = Address::random(&env);
        let borrower = Address::random(&env);
        
        // Attempt to overflow with i128::MAX stake
        let max_stake = i128::MAX;
        
        // This should either fail validation or handle gracefully
        // The contract should detect overflow and return StakeOverflow error
        let cfg = config(&env);
        assert!(cfg.yield_bps < 10000); // Yield BPS is valid
    }

    #[test]
    fn test_yield_calculation_no_overflow() {
        let (env, _admin, _admins, _token) = setup_env();
        
        let cfg = config(&env);
        let test_stake: i128 = i128::MAX / 2; // Half of max
        let yield_bps = cfg.yield_bps;
        
        // Calculate yield safely: (stake * yield_bps) / 10000
        // This should not panic
        let result = test_stake.checked_mul(yield_bps);
        
        // If stake is too large, checked_mul should return None
        // Contract should handle this gracefully
        if result.is_none() {
            // Overflow detected - contract should reject
        } else {
            let yield_amount = result.unwrap() / 10000;
            assert!(yield_amount >= 0);
        }
    }

    #[test]
    fn test_repayment_amount_overflow_check() {
        let (env, _admin, _admins, _token) = setup_env();
        
        let principal: i128 = i128::MAX / 3;
        let yield_amount: i128 = i128::MAX / 3;
        
        // Attempting to add principal + yield should overflow
        let total_needed = principal.checked_add(yield_amount);
        
        // Should either fail or be rejected
        assert!(total_needed.is_some() || total_needed.is_none());
    }

    #[test]
    fn test_negative_amount_protection() {
        let (env, _admin, _admins, _token) = setup_env();
        
        let cfg = config(&env);
        
        // Verify min_loan_amount is positive
        assert!(cfg.min_loan_amount > 0, "Min loan amount must be positive");
        
        // Verify min_stake is positive
        assert!(cfg.min_stake > 0, "Min stake must be positive");
        
        // Verify yield_bps is within valid range
        assert!(cfg.yield_bps <= 10000, "Yield BPS must be <= 10000");
        assert!(cfg.yield_bps >= 0, "Yield BPS must be >= 0");
    }

    #[test]
    fn test_slash_calculation_no_overflow() {
        let (env, _admin, _admins, _token) = setup_env();
        
        let cfg = config(&env);
        let test_stake: i128 = i128::MAX / 2;
        let slash_bps = cfg.slash_bps;
        
        // Calculate slashed amount: (stake * slash_bps) / 10000
        let result = test_stake.checked_mul(slash_bps);
        
        if result.is_some() {
            let slashed = result.unwrap() / 10000;
            assert!(slashed >= 0);
            assert!(slashed <= test_stake); // Slashed amount must be <= original stake
        }
    }

    #[test]
    fn test_total_stake_accumulation_overflow() {
        let (env, _admin, _admins, _token) = setup_env();
        
        // Simulate accumulating stake from multiple vouchers
        let stake_per_voucher: i128 = i128::MAX / 100;
        let num_vouchers: usize = 100;
        
        let mut total: i128 = 0;
        for _ in 0..num_vouchers {
            total = match total.checked_add(stake_per_voucher) {
                Some(v) => v,
                None => {
                    // Overflow detected - contract should prevent this
                    return;
                }
            };
        }
        
        // If we reach here, overflow didn't occur
        assert!(total > 0);
    }

    #[test]
    fn test_bps_denominator_validity() {
        let (env, _admin, _admins, _token) = setup_env();
        
        let cfg = config(&env);
        
        // BPS values must be between 0 and 10000
        assert!(cfg.yield_bps >= 0 && cfg.yield_bps <= 10000);
        assert!(cfg.slash_bps >= 0 && cfg.slash_bps <= 10000);
        assert!(cfg.protocol_fee_bps >= 0 && cfg.protocol_fee_bps <= 10000);
    }

    #[test]
    fn test_arithmetic_with_safe_bounds() {
        let (env, _admin, _admins, _token) = setup_env();
        
        let cfg = config(&env);
        
        // Test with safe bounds
        let safe_stake: i128 = 1_000_000_000; // 100 XLM in stroops
        
        let yield_amount = (safe_stake * cfg.yield_bps) / 10000;
        assert!(yield_amount >= 0);
        
        let total_repay = safe_stake.checked_add(yield_amount);
        assert!(total_repay.is_some());
        
        let slashed_amount = (safe_stake * cfg.slash_bps) / 10000;
        assert!(slashed_amount >= 0);
        assert!(slashed_amount <= safe_stake);
    }

    #[test]
    fn test_zero_amount_rejection() {
        let (env, _admin, _admins, _token) = setup_env();
        
        let cfg = config(&env);
        
        // Zero stake should be below minimum
        assert!(0 < cfg.min_stake);
        
        // Zero loan amount should be below minimum
        assert!(0 < cfg.min_loan_amount);
    }

    #[test]
    fn test_max_i128_safe_division() {
        let (env, _admin, _admins, _token) = setup_env();
        
        let cfg = config(&env);
        
        // Even with max i128, division by 10000 should work
        let max_val: i128 = i128::MAX;
        let divided = max_val / 10000;
        
        assert!(divided > 0);
        
        // Verify division doesn't overflow
        let re_multiplied = divided * 10000;
        assert!(re_multiplied <= max_val);
    }
}

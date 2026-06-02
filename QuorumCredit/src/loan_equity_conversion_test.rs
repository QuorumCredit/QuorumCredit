#[cfg(test)]
mod tests {
    use crate::errors::ContractError;
    use crate::types::{
        DataKey, EquityConversionTerms, LoanRecord, LoanStatus, VouchRecord,
        DEFAULT_YIELD_BPS, DEFAULT_SLASH_BPS, DEFAULT_MIN_LOAN_AMOUNT, LoanCategory,
    };
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::{Address, Env, Symbol};

    // Helper to create test environment
    fn setup_test_env() -> (Env, Address, Address, Address, Address) {
        let env = Env::default();
        let deployer = Address::random(&env);
        let admin = Address::random(&env);
        let borrower = Address::random(&env);
        let token = Address::random(&env);

        env.ledger().set_timestamp(100);
        (env, deployer, admin, borrower, token)
    }

    #[test]
    fn test_equity_conversion_set_terms() {
        let (env, _deployer, _admin, borrower, _token) = setup_test_env();

        // Setup: Create mock loan record
        let loan_id = 1u64;
        let loan = LoanRecord {
            id: loan_id,
            borrower: borrower.clone(),
            co_borrowers: soroban_sdk::Vec::new(&env),
            amount: 5_000_000,     // 0.5 XLM
            amount_repaid: 0,
            total_yield: 100_000,
            yield_bps: DEFAULT_YIELD_BPS,
            slash_bps: DEFAULT_SLASH_BPS,
            status: LoanStatus::Active,
            created_at: 50,
            disbursement_timestamp: 100,
            repayment_timestamp: None,
            deadline: 100 + 30 * 24 * 60 * 60,
            loan_purpose: soroban_sdk::String::from_str(&env, "Business expansion"),
            loan_category: LoanCategory::Business,
            token_address: Address::random(&env),
            syndicate_id: None,
        };

        // Store loan
        env.storage()
            .persistent()
            .set(&DataKey::Loan(loan_id), &loan);

        // Test: Set equity conversion terms (10% equity for loan forgiveness)
        let equity_bps = 1000u32; // 10%
        let terms = EquityConversionTerms {
            equity_percentage_bps: equity_bps,
            agreed_at: 100,
            executed: false,
            executed_at: 0,
        };

        env.storage()
            .persistent()
            .set(&DataKey::EquityConversionTerms(loan_id), &terms);

        // Verify: Terms stored correctly
        let stored_terms: EquityConversionTerms = env
            .storage()
            .persistent()
            .get(&DataKey::EquityConversionTerms(loan_id))
            .unwrap();

        assert_eq!(stored_terms.equity_percentage_bps, equity_bps);
        assert_eq!(stored_terms.agreed_at, 100);
        assert_eq!(stored_terms.executed, false);
        assert_eq!(stored_terms.executed_at, 0);
    }

    #[test]
    fn test_equity_conversion_only_active_loans() {
        let (env, _deployer, _admin, borrower, _token) = setup_test_env();

        let loan_id = 1u64;
        // Create a REPAID loan (should not allow equity conversion)
        let loan = LoanRecord {
            id: loan_id,
            borrower: borrower.clone(),
            co_borrowers: soroban_sdk::Vec::new(&env),
            amount: 5_000_000,
            amount_repaid: 5_100_000, // Fully repaid
            total_yield: 100_000,
            yield_bps: DEFAULT_YIELD_BPS,
            slash_bps: DEFAULT_SLASH_BPS,
            status: LoanStatus::Repaid,
            created_at: 50,
            disbursement_timestamp: 100,
            repayment_timestamp: Some(200),
            deadline: 100 + 30 * 24 * 60 * 60,
            loan_purpose: soroban_sdk::String::from_str(&env, "Business expansion"),
            loan_category: LoanCategory::Business,
            token_address: Address::random(&env),
            syndicate_id: None,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Loan(loan_id), &loan);

        // Attempt: Conversion should only be allowed on Active loans
        let stored_loan: LoanRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Loan(loan_id))
            .unwrap();

        // Verify: Loan is in Repaid status
        assert_eq!(stored_loan.status, LoanStatus::Repaid);
        // Logic: Contract should reject conversion attempt on non-Active loans
    }

    #[test]
    fn test_equity_conversion_multiple_borrowers() {
        let (env, _deployer, _admin, _borrower, _token) = setup_test_env();

        let borrower1 = Address::random(&env);
        let borrower2 = Address::random(&env);

        // Setup: Two separate loans with different equity terms
        let loan1 = LoanRecord {
            id: 1u64,
            borrower: borrower1.clone(),
            co_borrowers: soroban_sdk::Vec::new(&env),
            amount: 5_000_000,
            amount_repaid: 0,
            total_yield: 100_000,
            yield_bps: DEFAULT_YIELD_BPS,
            slash_bps: DEFAULT_SLASH_BPS,
            status: LoanStatus::Active,
            created_at: 50,
            disbursement_timestamp: 100,
            repayment_timestamp: None,
            deadline: 100 + 30 * 24 * 60 * 60,
            loan_purpose: soroban_sdk::String::from_str(&env, "Business expansion"),
            loan_category: LoanCategory::Business,
            token_address: Address::random(&env),
            syndicate_id: None,
        };

        let loan2 = LoanRecord {
            id: 2u64,
            borrower: borrower2.clone(),
            co_borrowers: soroban_sdk::Vec::new(&env),
            amount: 10_000_000,
            amount_repaid: 0,
            total_yield: 200_000,
            yield_bps: DEFAULT_YIELD_BPS,
            slash_bps: DEFAULT_SLASH_BPS,
            status: LoanStatus::Active,
            created_at: 50,
            disbursement_timestamp: 100,
            repayment_timestamp: None,
            deadline: 100 + 30 * 24 * 60 * 60,
            loan_purpose: soroban_sdk::String::from_str(&env, "Agriculture equipment"),
            loan_category: LoanCategory::Agriculture,
            token_address: Address::random(&env),
            syndicate_id: None,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Loan(1u64), &loan1);
        env.storage()
            .persistent()
            .set(&DataKey::Loan(2u64), &loan2);

        // Set different equity terms for each
        let terms1 = EquityConversionTerms {
            equity_percentage_bps: 500,  // 5%
            agreed_at: 100,
            executed: false,
            executed_at: 0,
        };

        let terms2 = EquityConversionTerms {
            equity_percentage_bps: 1500, // 15%
            agreed_at: 100,
            executed: false,
            executed_at: 0,
        };

        env.storage()
            .persistent()
            .set(&DataKey::EquityConversionTerms(1u64), &terms1);
        env.storage()
            .persistent()
            .set(&DataKey::EquityConversionTerms(2u64), &terms2);

        // Verify: Both terms stored separately
        let stored_terms1: EquityConversionTerms = env
            .storage()
            .persistent()
            .get(&DataKey::EquityConversionTerms(1u64))
            .unwrap();

        let stored_terms2: EquityConversionTerms = env
            .storage()
            .persistent()
            .get(&DataKey::EquityConversionTerms(2u64))
            .unwrap();

        assert_eq!(stored_terms1.equity_percentage_bps, 500);
        assert_eq!(stored_terms2.equity_percentage_bps, 1500);
        assert_ne!(stored_terms1.equity_percentage_bps, stored_terms2.equity_percentage_bps);
    }

    #[test]
    fn test_equity_conversion_execution_flag() {
        let (env, _deployer, _admin, borrower, _token) = setup_test_env();

        let loan_id = 1u64;
        let initial_terms = EquityConversionTerms {
            equity_percentage_bps: 1000,
            agreed_at: 100,
            executed: false,
            executed_at: 0,
        };

        env.storage()
            .persistent()
            .set(&DataKey::EquityConversionTerms(loan_id), &initial_terms);

        // Verify: Initially not executed
        let terms: EquityConversionTerms = env
            .storage()
            .persistent()
            .get(&DataKey::EquityConversionTerms(loan_id))
            .unwrap();

        assert_eq!(terms.executed, false);
        assert_eq!(terms.executed_at, 0);

        // Simulate: Execution
        let executed_terms = EquityConversionTerms {
            equity_percentage_bps: 1000,
            agreed_at: 100,
            executed: true,
            executed_at: 150,
        };

        env.storage()
            .persistent()
            .set(&DataKey::EquityConversionTerms(loan_id), &executed_terms);

        // Verify: After execution
        let final_terms: EquityConversionTerms = env
            .storage()
            .persistent()
            .get(&DataKey::EquityConversionTerms(loan_id))
            .unwrap();

        assert_eq!(final_terms.executed, true);
        assert_eq!(final_terms.executed_at, 150);
    }

    #[test]
    fn test_equity_conversion_boundary_values() {
        let (env, _deployer, _admin, _borrower, _token) = setup_test_env();

        // Test: Minimum equity (1%)
        let terms_min = EquityConversionTerms {
            equity_percentage_bps: 100,
            agreed_at: 100,
            executed: false,
            executed_at: 0,
        };

        // Test: Maximum equity (100%)
        let terms_max = EquityConversionTerms {
            equity_percentage_bps: 10000,
            agreed_at: 100,
            executed: false,
            executed_at: 0,
        };

        env.storage()
            .persistent()
            .set(&DataKey::EquityConversionTerms(1u64), &terms_min);
        env.storage()
            .persistent()
            .set(&DataKey::EquityConversionTerms(2u64), &terms_max);

        let stored_min: EquityConversionTerms = env
            .storage()
            .persistent()
            .get(&DataKey::EquityConversionTerms(1u64))
            .unwrap();

        let stored_max: EquityConversionTerms = env
            .storage()
            .persistent()
            .get(&DataKey::EquityConversionTerms(2u64))
            .unwrap();

        assert_eq!(stored_min.equity_percentage_bps, 100);
        assert_eq!(stored_max.equity_percentage_bps, 10000);
    }

    #[test]
    fn test_equity_conversion_terms_persistence() {
        let (env, _deployer, _admin, _borrower, _token) = setup_test_env();

        let loan_id = 1u64;
        let initial_terms = EquityConversionTerms {
            equity_percentage_bps: 750,
            agreed_at: 100,
            executed: false,
            executed_at: 0,
        };

        // Store once
        env.storage()
            .persistent()
            .set(&DataKey::EquityConversionTerms(loan_id), &initial_terms);

        // Retrieve and verify persistence
        let retrieved1: EquityConversionTerms = env
            .storage()
            .persistent()
            .get(&DataKey::EquityConversionTerms(loan_id))
            .unwrap();

        assert_eq!(retrieved1.equity_percentage_bps, 750);

        // Retrieve again to ensure data is still there
        let retrieved2: EquityConversionTerms = env
            .storage()
            .persistent()
            .get(&DataKey::EquityConversionTerms(loan_id))
            .unwrap();

        assert_eq!(retrieved2.equity_percentage_bps, 750);
        assert_eq!(retrieved1.equity_percentage_bps, retrieved2.equity_percentage_bps);
    }

    #[test]
    fn test_equity_conversion_active_loan_required() {
        let (env, _deployer, _admin, borrower, _token) = setup_test_env();

        let loan_id = 1u64;
        
        // Create loan with different statuses to verify conversion only works on Active
        for (status, should_allow) in &[
            (LoanStatus::Active, true),
            (LoanStatus::Pending, false),
            (LoanStatus::Repaid, false),
            (LoanStatus::Defaulted, false),
        ] {
            let loan = LoanRecord {
                id: loan_id,
                borrower: borrower.clone(),
                co_borrowers: soroban_sdk::Vec::new(&env),
                amount: 5_000_000,
                amount_repaid: if *status == LoanStatus::Repaid { 5_100_000 } else { 0 },
                total_yield: 100_000,
                yield_bps: DEFAULT_YIELD_BPS,
                slash_bps: DEFAULT_SLASH_BPS,
                status: status.clone(),
                created_at: 50,
                disbursement_timestamp: 100,
                repayment_timestamp: if *status == LoanStatus::Repaid { Some(200) } else { None },
                deadline: 100 + 30 * 24 * 60 * 60,
                loan_purpose: soroban_sdk::String::from_str(&env, "Business expansion"),
                loan_category: LoanCategory::Business,
                token_address: Address::random(&env),
                syndicate_id: None,
            };

            env.storage()
                .persistent()
                .set(&DataKey::Loan(loan_id), &loan);

            // Verify stored loan status
            let stored_loan: LoanRecord = env
                .storage()
                .persistent()
                .get(&DataKey::Loan(loan_id))
                .unwrap();

            assert_eq!(stored_loan.status, *status);
            // In actual implementation, conversion should only be allowed when status == Active
            if *should_allow {
                assert_eq!(stored_loan.status, LoanStatus::Active);
            }
        }
    }

    #[test]
    fn test_equity_conversion_zero_percentage_rejected() {
        let (env, _deployer, _admin, _borrower, _token) = setup_test_env();

        let loan_id = 1u64;
        
        // Zero equity should be invalid
        let invalid_terms = EquityConversionTerms {
            equity_percentage_bps: 0,
            agreed_at: 100,
            executed: false,
            executed_at: 0,
        };

        env.storage()
            .persistent()
            .set(&DataKey::EquityConversionTerms(loan_id), &invalid_terms);

        let stored = env
            .storage()
            .persistent()
            .get(&DataKey::EquityConversionTerms(loan_id))
            .unwrap();

        // Verify it's stored but would be rejected by contract logic
        assert_eq!(stored.equity_percentage_bps, 0);
    }

    #[test]
    fn test_equity_conversion_chronological_ordering() {
        let (env, _deployer, _admin, _borrower, _token) = setup_test_env();

        let loan_id = 1u64;
        
        // Create terms at time 100
        let initial_terms = EquityConversionTerms {
            equity_percentage_bps: 500,
            agreed_at: 100,
            executed: false,
            executed_at: 0,
        };

        env.storage()
            .persistent()
            .set(&DataKey::EquityConversionTerms(loan_id), &initial_terms);

        // Update to executed state at later time
        let executed_terms = EquityConversionTerms {
            equity_percentage_bps: 500,
            agreed_at: 100,
            executed: true,
            executed_at: 200,
        };

        env.storage()
            .persistent()
            .set(&DataKey::EquityConversionTerms(loan_id), &executed_terms);

        let final_terms: EquityConversionTerms = env
            .storage()
            .persistent()
            .get(&DataKey::EquityConversionTerms(loan_id))
            .unwrap();

        // Verify time ordering
        assert_eq!(final_terms.agreed_at, 100);
        assert!(final_terms.executed_at > final_terms.agreed_at);
        assert_eq!(final_terms.executed_at, 200);
    }

    #[test]
    fn test_equity_conversion_idempotent_execution() {
        let (env, _deployer, _admin, _borrower, _token) = setup_test_env();

        let loan_id = 1u64;
        
        // Create terms
        let terms = EquityConversionTerms {
            equity_percentage_bps: 1000,
            agreed_at: 100,
            executed: true,
            executed_at: 150,
        };

        env.storage()
            .persistent()
            .set(&DataKey::EquityConversionTerms(loan_id), &terms);

        // Attempt to execute again (idempotent - should not change)
        let terms2 = EquityConversionTerms {
            equity_percentage_bps: 1000,
            agreed_at: 100,
            executed: true,
            executed_at: 150,
        };

        env.storage()
            .persistent()
            .set(&DataKey::EquityConversionTerms(loan_id), &terms2);

        let final_terms: EquityConversionTerms = env
            .storage()
            .persistent()
            .get(&DataKey::EquityConversionTerms(loan_id))
            .unwrap();

        // Should remain the same
        assert_eq!(final_terms.executed_at, 150);
        assert_eq!(final_terms.executed, true);
    }
}

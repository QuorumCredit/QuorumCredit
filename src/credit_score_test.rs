#![cfg(test)]

use crate::credit_score::{
    calculate_credit_score, calculate_timeliness_score, calculate_repayment_history_score,
    calculate_loan_count_score, calculate_account_age_score, calculate_vouching_score,
};
use crate::types::{
    CreditScore, CreditTier, DataKey, LoanRecord, LoanStatus, PaymentRecord,
    DEFAULT_CREDIT_SCORE_CONFIG,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, Vec,
};

#[test]
fn test_timeliness_score_early_repayment() {
    // 5 days early = 432000 seconds
    let early_secs: i64 = 5 * 24 * 60 * 60;
    let score = calculate_timeliness_score(early_secs);
    assert!(score > 500, "Early repayment should score > 500, got {}", score);
}

#[test]
fn test_timeliness_score_late_repayment() {
    // 3 days late = -259200 seconds
    let late_secs: i64 = -3 * 24 * 60 * 60;
    let score = calculate_timeliness_score(late_secs);
    assert!(score < 500, "Late repayment should score < 500, got {}", score);
}

#[test]
fn test_timeliness_score_neutral() {
    let score = calculate_timeliness_score(0);
    assert_eq!(score, 500, "Neutral timeliness should score 500");
}

#[test]
fn test_timeliness_score_very_early() {
    // 7+ days early = max score
    let very_early_secs: i64 = 8 * 24 * 60 * 60;
    let score = calculate_timeliness_score(very_early_secs);
    assert_eq!(score, 1000, "Very early repayment should score 1000");
}

#[test]
fn test_timeliness_score_very_late() {
    // 7+ days late = min score
    let very_late_secs: i64 = -8 * 24 * 60 * 60;
    let score = calculate_timeliness_score(very_late_secs);
    assert_eq!(score, 0, "Very late repayment should score 0");
}

#[test]
fn test_repayment_history_score_perfect() {
    // 5 successful out of 5 loans, no defaults
    let score = calculate_repayment_history_score(5, 5, 0);
    assert_eq!(score, 1000, "Perfect repayment should score 1000");
}

#[test]
fn test_repayment_history_score_with_defaults() {
    // 5 successful out of 6 loans, 1 default
    // success_rate = 5/6 * 1000 = 833
    // penalty = 1 * 200 = 200
    // adjusted = 833 - 200 = 633
    let score = calculate_repayment_history_score(5, 6, 1);
    assert_eq!(score, 633, "5/6 with 1 default should score 633");
}

#[test]
fn test_repayment_history_score_new_user() {
    // New user with no loans
    let score = calculate_repayment_history_score(0, 0, 0);
    assert_eq!(score, 500, "New user should score 500 (neutral)");
}

#[test]
fn test_loan_count_score() {
    // Max benefit at 10 loans
    let score_5 = calculate_loan_count_score(5);
    let score_10 = calculate_loan_count_score(10);
    let score_15 = calculate_loan_count_score(15);
    
    assert!(score_5 < score_10, "5 loans should score < 10 loans");
    assert_eq!(score_10, 1000, "10 loans should score 1000 (max)");
    assert_eq!(score_15, 1000, "15 loans capped at 1000");
}

#[test]
fn test_account_age_score() {
    // Max benefit at 1 year
    let one_year = 365 * 24 * 60 * 60;
    let score_1_year = calculate_account_age_score(one_year);
    let score_half_year = calculate_account_age_score(one_year / 2);
    
    assert_eq!(score_1_year, 1000, "1 year account age should score 1000");
    assert_eq!(score_half_year, 500, "0.5 year account age should score 500");
}

#[test]
fn test_vouching_score() {
    // Max benefit at 20 vouches
    let score_10 = calculate_vouching_score(10);
    let score_20 = calculate_vouching_score(20);
    let score_30 = calculate_vouching_score(30);
    
    assert!(score_10 < score_20, "10 vouches should score < 20 vouches");
    assert_eq!(score_20, 1000, "20 vouches should score 1000 (max)");
    assert_eq!(score_30, 1000, "30 vouches capped at 1000");
}

#[test]
fn test_different_repayment_histories_produce_different_scores() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuorumCreditContract, ());
    env.as_contract(&contract_id, || {
    let borrower_early = Address::generate(&env);
    let borrower_late = Address::generate(&env);

    // Initialize credit score config
    env.storage()
        .instance()
        .set(&DataKey::CreditScoreConfig, &DEFAULT_CREDIT_SCORE_CONFIG);

    // Borrower with early repayments
    // Create a loan repaid early
    let loan_id_early = 1u64;
    let now = env.ledger().timestamp();
    let deadline = now + 100_000; // Far in future
    
    let loan_early = LoanRecord {
        id: loan_id_early,
        borrower: borrower_early.clone(),
        guarantor: None,
        buyback_price: 0,
        auto_repay_enabled: false,
        auto_repay_attempts: 0,
        escrow_status: crate::types::EscrowStatus::None,
        co_borrowers: Vec::new(&env),
        amount: 1_000_000,
        amount_repaid: 1_000_000,
        total_yield: 20_000,
        status: LoanStatus::Repaid,
        repaid: true,
        defaulted: false,
        created_at: now,
        disbursement_timestamp: now,
        repayment_timestamp: Some(now + 10_000), // Repaid 10k secs early (early)
        deadline,
        loan_purpose: soroban_sdk::String::from_str(&env, "test"),
        token_address: Address::generate(&env),
        amortization_schedule: Vec::new(&env),
        reminder_sent: false,
        risk_score: 50,
        deferment_periods: 0,
        maturity_date: None,
        rate_type: crate::types::RateType::Fixed,
        index_reference: None,
        last_interest_calc: now,
        accrued_interest: 0,
        milestone_bonus_applied: false,
        retry_count: 0,
        suspension_timestamp: None,
        suspension_amount_repaid: 0,
    };

    // Borrower with late repayments
    let loan_id_late = 2u64;
    let loan_late = LoanRecord {
        id: loan_id_late,
        borrower: borrower_late.clone(),
        guarantor: None,
        buyback_price: 0,
        auto_repay_enabled: false,
        auto_repay_attempts: 0,
        escrow_status: crate::types::EscrowStatus::None,
        co_borrowers: Vec::new(&env),
        amount: 1_000_000,
        amount_repaid: 1_000_000,
        total_yield: 20_000,
        status: LoanStatus::Repaid,
        repaid: true,
        defaulted: false,
        created_at: now,
        disbursement_timestamp: now,
        repayment_timestamp: Some(deadline + 10_000), // Repaid 10k secs late (late)
        deadline,
        loan_purpose: soroban_sdk::String::from_str(&env, "test"),
        token_address: Address::generate(&env),
        amortization_schedule: Vec::new(&env),
        reminder_sent: false,
        risk_score: 50,
        deferment_periods: 0,
        maturity_date: None,
        rate_type: crate::types::RateType::Fixed,
        index_reference: None,
        last_interest_calc: now,
        accrued_interest: 0,
        milestone_bonus_applied: false,
        retry_count: 0,
        suspension_timestamp: None,
        suspension_amount_repaid: 0,
    };

    // Store loans
    env.storage()
        .persistent()
        .set(&DataKey::Loan(loan_id_early), &loan_early);
    env.storage()
        .persistent()
        .set(&DataKey::Loan(loan_id_late), &loan_late);

    // Set loan counter
    env.storage()
        .persistent()
        .set(&DataKey::LoanCounter, &(2u64));

    // Set loan counts
    env.storage()
        .persistent()
        .set(&DataKey::LoanCount(borrower_early.clone()), &(1u32));
    env.storage()
        .persistent()
        .set(&DataKey::LoanCount(borrower_late.clone()), &(1u32));

    // Set repayment counts
    env.storage()
        .persistent()
        .set(&DataKey::RepaymentCount(borrower_early.clone()), &(1u32));
    env.storage()
        .persistent()
        .set(&DataKey::RepaymentCount(borrower_late.clone()), &(1u32));

    // Set registration timestamps (same account age)
    let registration_time = now - 30_000_000; // ~1 year old
    env.storage()
        .persistent()
        .set(&DataKey::BorrowerRegistered(borrower_early.clone()), &registration_time);
    env.storage()
        .persistent()
        .set(&DataKey::BorrowerRegistered(borrower_late.clone()), &registration_time);

    // Calculate credit scores
    let score_early = calculate_credit_score(&env, &borrower_early)
        .expect("Failed to calculate early repayment score");
    let score_late = calculate_credit_score(&env, &borrower_late)
        .expect("Failed to calculate late repayment score");

    // Early repayer should have higher score than late repayer
    assert!(
        score_early.score > score_late.score,
        "Early repayment score ({}) should be > late repayment score ({})",
        score_early.score,
        score_late.score
    );
    
    // Early repayer should have positive avg_repayment_time
    assert!(
        score_early.avg_repayment_time > 0,
        "Early repayment avg_repayment_time ({}) should be positive",
        score_early.avg_repayment_time
    );
    
    // Late repayer should have negative avg_repayment_time
    assert!(
        score_late.avg_repayment_time < 0,
        "Late repayment avg_repayment_time ({}) should be negative",
        score_late.avg_repayment_time
    );
    });
}

#[test]
fn test_credit_score_total_borrowed() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuorumCreditContract, ());
    env.as_contract(&contract_id, || {
    let borrower = Address::generate(&env);
    let now = env.ledger().timestamp();

    // Initialize credit score config
    env.storage()
        .instance()
        .set(&DataKey::CreditScoreConfig, &DEFAULT_CREDIT_SCORE_CONFIG);

    // Create two loans
    let loan1 = LoanRecord {
        id: 1u64,
        borrower: borrower.clone(),
        guarantor: None,
        buyback_price: 0,
        auto_repay_enabled: false,
        auto_repay_attempts: 0,
        escrow_status: crate::types::EscrowStatus::None,
        co_borrowers: Vec::new(&env),
        amount: 500_000,
        amount_repaid: 500_000,
        total_yield: 10_000,
        status: LoanStatus::Repaid,
        repaid: true,
        defaulted: false,
        created_at: now,
        disbursement_timestamp: now,
        repayment_timestamp: Some(now + 50_000),
        deadline: now + 100_000,
        loan_purpose: soroban_sdk::String::from_str(&env, "test1"),
        token_address: Address::generate(&env),
        amortization_schedule: Vec::new(&env),
        reminder_sent: false,
        risk_score: 50,
        deferment_periods: 0,
        maturity_date: None,
        rate_type: crate::types::RateType::Fixed,
        index_reference: None,
        last_interest_calc: now,
        accrued_interest: 0,
        milestone_bonus_applied: false,
        retry_count: 0,
        suspension_timestamp: None,
        suspension_amount_repaid: 0,
    };

    let loan2 = LoanRecord {
        id: 2u64,
        borrower: borrower.clone(),
        guarantor: None,
        buyback_price: 0,
        auto_repay_enabled: false,
        auto_repay_attempts: 0,
        escrow_status: crate::types::EscrowStatus::None,
        co_borrowers: Vec::new(&env),
        amount: 300_000,
        amount_repaid: 300_000,
        total_yield: 6_000,
        status: LoanStatus::Repaid,
        repaid: true,
        defaulted: false,
        created_at: now,
        disbursement_timestamp: now,
        repayment_timestamp: Some(now + 50_000),
        deadline: now + 100_000,
        loan_purpose: soroban_sdk::String::from_str(&env, "test2"),
        token_address: Address::generate(&env),
        amortization_schedule: Vec::new(&env),
        reminder_sent: false,
        risk_score: 50,
        deferment_periods: 0,
        maturity_date: None,
        rate_type: crate::types::RateType::Fixed,
        index_reference: None,
        last_interest_calc: now,
        accrued_interest: 0,
        milestone_bonus_applied: false,
        retry_count: 0,
        suspension_timestamp: None,
        suspension_amount_repaid: 0,
    };

    // Store loans
    env.storage().persistent().set(&DataKey::Loan(1u64), &loan1);
    env.storage().persistent().set(&DataKey::Loan(2u64), &loan2);
    env.storage()
        .persistent()
        .set(&DataKey::LoanCounter, &(2u64));

    // Set counts
    env.storage()
        .persistent()
        .set(&DataKey::LoanCount(borrower.clone()), &(2u32));
    env.storage()
        .persistent()
        .set(&DataKey::RepaymentCount(borrower.clone()), &(2u32));
    env.storage()
        .persistent()
        .set(&DataKey::BorrowerRegistered(borrower.clone()), &now);

    let credit_score = calculate_credit_score(&env, &borrower)
        .expect("Failed to calculate credit score");

    // Total borrowed should be 500_000 + 300_000 = 800_000
    assert_eq!(
        credit_score.total_borrowed, 800_000,
        "Total borrowed should be 800_000"
    );
    });
}

#[test]
fn test_credit_score_total_repaid() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuorumCreditContract, ());
    env.as_contract(&contract_id, || {
    let borrower = Address::generate(&env);
    let now = env.ledger().timestamp();

    // Initialize credit score config
    env.storage()
        .instance()
        .set(&DataKey::CreditScoreConfig, &DEFAULT_CREDIT_SCORE_CONFIG);

    // Create a loan with partial repayment
    let loan = LoanRecord {
        id: 1u64,
        borrower: borrower.clone(),
        guarantor: None,
        buyback_price: 0,
        auto_repay_enabled: false,
        auto_repay_attempts: 0,
        escrow_status: crate::types::EscrowStatus::None,
        co_borrowers: Vec::new(&env),
        amount: 1_000_000,
        amount_repaid: 750_000, // Partial repayment
        total_yield: 20_000,
        status: LoanStatus::Active,
        repaid: false,
        defaulted: false,
        created_at: now,
        disbursement_timestamp: now,
        repayment_timestamp: None,
        deadline: now + 100_000,
        loan_purpose: soroban_sdk::String::from_str(&env, "test"),
        token_address: Address::generate(&env),
        amortization_schedule: Vec::new(&env),
        reminder_sent: false,
        risk_score: 50,
        deferment_periods: 0,
        maturity_date: None,
        rate_type: crate::types::RateType::Fixed,
        index_reference: None,
        last_interest_calc: now,
        accrued_interest: 0,
        milestone_bonus_applied: false,
        retry_count: 0,
        suspension_timestamp: None,
        suspension_amount_repaid: 0,
    };

    env.storage().persistent().set(&DataKey::Loan(1u64), &loan);
    env.storage()
        .persistent()
        .set(&DataKey::LoanCounter, &(1u64));

    env.storage()
        .persistent()
        .set(&DataKey::LoanCount(borrower.clone()), &(1u32));
    env.storage()
        .persistent()
        .set(&DataKey::RepaymentCount(borrower.clone()), &(0u32));
    env.storage()
        .persistent()
        .set(&DataKey::BorrowerRegistered(borrower.clone()), &now);

    let credit_score = calculate_credit_score(&env, &borrower)
        .expect("Failed to calculate credit score");

    // Total repaid should be 750_000
    assert_eq!(
        credit_score.total_repaid, 750_000,
        "Total repaid should be 750_000"
    );
    });
}

#[test]
fn test_credit_score_migration_strategy_note() {
    // This test documents the migration strategy for existing borrowers
    // Currently, there is NO HISTORICAL DATA for borrowers on the old contract
    // They will have:
    // - total_borrowed: 0 (since they may have had loans, but loan records don't predate)
    // - total_repaid: 0 (same)
    // - avg_repayment_time: 0 (neutral, no history)
    // 
    // Migration path:
    // 1. Any existing borrower's historical loans should be backfilled from off-chain data or contract logs
    // 2. Create LoanRecord entries for past loans with accurate:
    //    - disbursement_timestamp
    //    - repayment_timestamp
    //    - amount
    //    - status (Repaid or Defaulted)
    // 3. This populates their aggregates correctly going forward
    // 4. For borrowers with no off-chain history, they restart with neutral score (500) and build from new loans
    
    // Until backfill is implemented, all credit scores will be based on post-upgrade activity only
}

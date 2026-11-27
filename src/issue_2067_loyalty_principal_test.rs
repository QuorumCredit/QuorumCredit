//! Issue #2067: the anniversary bonus must be computed from the user's own loan record,
//! not from a caller-supplied principal.

#![cfg(test)]

use crate::errors::ContractError;
use crate::loyalty::{benefits_for_tier, claim_anniversary_bonus};
use crate::types::{
    DataKey, LoanRecord, LoanStatus, LoyaltyRecord, LoyaltyTier, LOYALTY_ANNIVERSARY_PERIOD_SECS,
};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::{Address as _, Ledger}, token::StellarAssetClient, Address, Env, Vec};

const LOAN_ID: u64 = 7;
const LOAN_AMOUNT: i128 = 1_000_000;

fn initialized_contract(env: &Env) -> Address {
    let deployer = Address::generate(env);
    let admin = Address::generate(env);
    let admins = Vec::from_array(env, [admin.clone()]);
    let token = env.register_stellar_asset_contract_v2(admin).address();
    let contract_id = env.register(QuorumCreditContract, ());
    StellarAssetClient::new(env, &token).mint(&contract_id, &1_000_000);
    let client = QuorumCreditContractClient::new(env, &contract_id);
    client.initialize(&deployer, &admins, &1u32, &token);
    contract_id
}

fn store_loan(env: &Env, contract_id: &Address, borrower: &Address, amount: i128) {
    let now = env.ledger().timestamp();
    let loan = LoanRecord {
        id: LOAN_ID,
        borrower: borrower.clone(),
        guarantor: None,
        buyback_price: 0,
        auto_repay_enabled: false,
        auto_repay_attempts: 0,
        escrow_status: crate::types::EscrowStatus::None,
        co_borrowers: Vec::new(env),
        amount,
        amount_repaid: 0,
        total_yield: 0,
        status: LoanStatus::Active,
        repaid: false,
        defaulted: false,
        created_at: now,
        disbursement_timestamp: now,
        repayment_timestamp: None,
        deadline: now + 100_000,
        loan_purpose: soroban_sdk::String::from_str(env, "test"),
        token_address: Address::generate(env),
        amortization_schedule: Vec::new(env),
        reminder_sent: false,
        risk_score: 50,
        deferment_periods: 0,
        maturity_date: None,
        rate_type: crate::types::RateType::Fixed,
        index_reference: None,
        last_interest_calc: now,
        accrued_interest: 0,
        milestone_bonus_applied: 0,
        retry_count: 0,
        suspension_timestamp: None,
        suspension_amount_repaid: 0,
    };
    env.as_contract(contract_id, || {
        env.storage().persistent().set(&DataKey::Loan(LOAN_ID), &loan);
        env.storage()
            .persistent()
            .set(&DataKey::LatestLoan(borrower.clone()), &LOAN_ID);
        // Gold tier, member for a full year, so the bonus is eligible.
        let record = LoyaltyRecord {
            user: borrower.clone(),
            tier: LoyaltyTier::Gold,
            repayment_count: 20,
            member_since: 0,
            last_tier_upgrade_at: 0,
            last_anniversary_bonus_at: 0,
            total_benefits_earned: 0,
        };
        env.storage()
            .persistent()
            .set(&DataKey::LoyaltyRecord(borrower.clone()), &record);
    });
}

fn benefits_total(env: &Env, contract_id: &Address, borrower: &Address) -> i128 {
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .get::<_, LoyaltyRecord>(&DataKey::LoyaltyRecord(borrower.clone()))
            .expect("loyalty record")
            .total_benefits_earned
    })
}

fn setup() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger()
        .with_mut(|l| l.timestamp = LOYALTY_ANNIVERSARY_PERIOD_SECS + 120);
    let contract_id = initialized_contract(&env);
    let borrower = Address::generate(&env);
    (env, contract_id, borrower)
}

#[test]
fn test_mismatched_principal_is_refused_and_benefits_unchanged() {
    let (env, contract_id, borrower) = setup();
    store_loan(&env, &contract_id, &borrower, LOAN_AMOUNT);

    // Caller claims a principal five times the real loan.
    let result = env.as_contract(&contract_id, || {
        claim_anniversary_bonus(env.clone(), borrower.clone(), LOAN_AMOUNT * 5)
    });
    assert_eq!(result, Err(ContractError::InvalidAmount));
    assert_eq!(benefits_total(&env, &contract_id, &borrower), 0);
}

#[test]
fn test_user_without_loan_cannot_claim_with_invented_principal() {
    let (env, contract_id, borrower) = setup();
    // No loan stored for this borrower at all, only a loyalty record.
    env.as_contract(&contract_id, || {
        let record = LoyaltyRecord {
            user: borrower.clone(),
            tier: LoyaltyTier::Gold,
            repayment_count: 20,
            member_since: 0,
            last_tier_upgrade_at: 0,
            last_anniversary_bonus_at: 0,
            total_benefits_earned: 0,
        };
        env.storage()
            .persistent()
            .set(&DataKey::LoyaltyRecord(borrower.clone()), &record);
    });

    let result = env.as_contract(&contract_id, || {
        claim_anniversary_bonus(env.clone(), borrower.clone(), LOAN_AMOUNT)
    });
    assert_eq!(result, Err(ContractError::InvalidAmount));
    assert_eq!(benefits_total(&env, &contract_id, &borrower), 0);
}

#[test]
fn test_matching_principal_accrues_benefit_from_loan_record() {
    let (env, contract_id, borrower) = setup();
    store_loan(&env, &contract_id, &borrower, LOAN_AMOUNT);

    let bps = benefits_for_tier(LoyaltyTier::Gold).anniversary_bonus_bps;
    let result = env.as_contract(&contract_id, || {
        claim_anniversary_bonus(env.clone(), borrower.clone(), LOAN_AMOUNT)
    });
    assert_eq!(result, Ok(bps));
    assert_eq!(
        benefits_total(&env, &contract_id, &borrower),
        LOAN_AMOUNT * bps as i128 / 10_000
    );
}

#[test]
fn anniversary_claim_reports_ineligibility_and_next_timestamp() {
    let (env, contract_id, borrower) = setup();
    store_loan(&env, &contract_id, &borrower, LOAN_AMOUNT);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    env.ledger().with_mut(|l| l.timestamp = LOYALTY_ANNIVERSARY_PERIOD_SECS - 1);
    assert_eq!(
        client.try_claim_anniversary_bonus(&borrower, &LOAN_AMOUNT),
        Err(Ok(ContractError::NotEligibleYet))
    );
    assert_eq!(client.anniversary_bonus_eligible_at(&borrower), Some(LOYALTY_ANNIVERSARY_PERIOD_SECS));
    assert_eq!(benefits_total(&env, &contract_id, &borrower), 0);

    env.ledger().with_mut(|l| l.timestamp = LOYALTY_ANNIVERSARY_PERIOD_SECS);
    assert_eq!(
        client.claim_anniversary_bonus(&borrower, &LOAN_AMOUNT),
        benefits_for_tier(LoyaltyTier::Gold).anniversary_bonus_bps
    );
    assert_eq!(client.anniversary_bonus_eligible_at(&borrower), Some(2 * LOYALTY_ANNIVERSARY_PERIOD_SECS));
}

#[test]
fn only_the_borrower_can_initialize_covenants_and_only_admins_can_monitor() {
    let (env, contract_id, borrower) = setup();
    store_loan(&env, &contract_id, &borrower, LOAN_AMOUNT);
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::ActiveLoan(borrower.clone()), &LOAN_ID);
    });
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let stranger = Address::generate(&env);
    let types = Vec::new(&env);

    env.set_auths(&[]);
    assert!(matches!(
        client.try_initialize_loan_covenants(&borrower, &types, &5000, &5000, &0, &0, &60, &1),
        Err(Err(_))
    ));

    env.mock_all_auths();
    client.initialize_loan_covenants(&borrower, &types, &5000, &5000, &0, &0, &60, &1);
    let strangers = Vec::from_array(&env, [stranger]);
    assert!(matches!(
        client.try_monitor_loan_covenants(&strangers, &borrower),
        Err(Err(_))
    ));

    let admins = env.as_contract(&contract_id, || crate::helpers::config(&env).admins);
    env.set_auths(&[]);
    assert!(matches!(
        client.try_monitor_loan_covenants(&admins, &borrower),
        Err(Err(_))
    ));
    env.mock_all_auths();
    assert_eq!(client.monitor_loan_covenants(&admins, &borrower).loan_id, LOAN_ID);
}

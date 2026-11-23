//! Tests for issues #2140, #2141, #2142, and #2143.

use crate::errors::ContractError;
use crate::types::DataKey;
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::StellarAssetClient,
    Address, Env, String, Vec,
};

fn setup(env: &Env) -> (Address, Address, Address) {
    env.mock_all_auths();
    let deployer = Address::generate(env);
    let admin = Address::generate(env);
    let admins = Vec::from_array(env, [admin.clone()]);
    let token_id = env.register_stellar_asset_contract_v2(admin.clone());
    let contract_id = env.register_contract(None, QuorumCreditContract);
    StellarAssetClient::new(env, &token_id.address()).mint(&contract_id, &10_000_000_000);
    let client = QuorumCreditContractClient::new(env, &contract_id);
    client.initialize(&deployer, &admins, &1, &token_id.address());
    (contract_id, token_id.address(), admin)
}

// ── Issue #2140: Config patch flow ───────────────────────────────────────────

#[test]
fn config_patch_enqueue_and_apply() {
    let env = Env::default();
    let (contract_id, _token, admin) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let admin_signers = Vec::from_array(&env, [admin.clone()]);

    // No patches yet.
    assert_eq!(client.get_config_patch_count(), 0);
    assert!(!client.apply_next_config_patch());

    // Enqueue a patch with apply_after = 1000.
    client.enqueue_config_patch(
        &admin_signers,
        &crate::types::ConfigField::Dummy,
        &0i128,
        &1_000u64,
    );
    assert_eq!(client.get_config_patch_count(), 1);

    // Patch is not yet due (ledger time is 0).
    assert!(!client.apply_next_config_patch());
    assert_eq!(client.get_config_patch_count(), 1);

    // Advance ledger past apply_after.
    env.ledger().set_timestamp(2_000);
    assert!(client.apply_next_config_patch());
    assert_eq!(client.get_config_patch_count(), 0);
}

#[test]
fn get_config_patch_returns_patch_by_index() {
    let env = Env::default();
    let (contract_id, _token, admin) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let admin_signers = Vec::from_array(&env, [admin.clone()]);

    client.enqueue_config_patch(
        &admin_signers,
        &crate::types::ConfigField::Dummy,
        &42i128,
        &5_000u64,
    );

    let patch = client.get_config_patch(&0u32).unwrap();
    assert_eq!(patch.new_value, 42i128);
    assert_eq!(patch.apply_after, 5_000u64);

    // Out of range returns None.
    assert!(client.get_config_patch(&1u32).is_none());
}

// ── Issue #2141: vouch_with_sector stores the sector ─────────────────────────

#[test]
fn vouch_with_sector_stores_sector_label() {
    let env = Env::default();
    let (contract_id, token, _admin) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let voucher = Address::generate(&env);
    let borrower = Address::generate(&env);
    let stake = 1_000_000i128;

    StellarAssetClient::new(&env, &token).mint(&voucher, &stake);

    let sector = String::from_str(&env, "real-estate");
    client
        .vouch_with_sector(&voucher, &borrower, &stake, &token, &sector)
        .unwrap();

    // Verify the vouch exists.
    assert!(client.vouch_exists(&voucher, &borrower));

    // Verify the sector label was stored.
    let stored: Option<String> = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get(&DataKey::VouchSector(voucher.clone(), borrower.clone()))
    });
    assert_eq!(stored, Some(sector));
}

// ── Issue #2142: u32 counter overflow ────────────────────────────────────────

#[test]
fn lazy_default_count_overflow_returns_error() {
    use crate::lazy_default_detection::{check_and_mark_default, record_borrower_loan_id};
    use crate::types::{EscrowStatus, LoanRecord, LoanStatus, RateType};

    let env = Env::default();
    let contract_id = env.register_contract(None, QuorumCreditContract);
    env.ledger().set_timestamp(10_000);

    env.as_contract(&contract_id, || {
        let borrower = Address::generate(&env);

        // Pre-load the counter at u32::MAX so the next increment overflows.
        env.storage()
            .persistent()
            .set(&DataKey::DefaultCount(borrower.clone()), &u32::MAX);

        let loan = LoanRecord {
            id: 99,
            borrower: borrower.clone(),
            guarantor: None,
            buyback_price: 0,
            auto_repay_enabled: false,
            auto_repay_attempts: 0,
            escrow_status: EscrowStatus::None,
            co_borrowers: soroban_sdk::Vec::new(&env),
            amount: 1_000,
            amount_repaid: 0,
            total_yield: 0,
            status: LoanStatus::Active,
            repaid: false,
            defaulted: false,
            created_at: 0,
            disbursement_timestamp: 0,
            repayment_timestamp: None,
            deadline: 1, // past deadline → defaulted
            loan_purpose: soroban_sdk::String::from_str(&env, "test"),
            token_address: Address::generate(&env),
            amortization_schedule: soroban_sdk::Vec::new(&env),
            reminder_sent: false,
            risk_score: 0,
            deferment_periods: 0,
            maturity_date: None,
            rate_type: RateType::Fixed,
            index_reference: None,
            last_interest_calc: 0,
            accrued_interest: 0,
            milestone_bonus_applied: 0,
            retry_count: 0,
            suspension_timestamp: None,
            suspension_amount_repaid: 0,
        };
        env.storage().persistent().set(&DataKey::Loan(loan.id), &loan);
        record_borrower_loan_id(&env, &borrower, loan.id);

        let result = check_and_mark_default(&env, 99);
        assert_eq!(result, Err(ContractError::ArithmeticOverflow));
    });
}

#[test]
fn reputation_mint_saturates_at_u32_max() {
    use crate::reputation::{RepKey, ReputationNftContract};

    let env = Env::default();
    env.mock_all_auths();
    let minter = Address::generate(&env);
    let holder = Address::generate(&env);

    let nft_id = env.register_contract(None, ReputationNftContract);
    let nft = crate::reputation::ReputationNftContractClient::new(&env, &nft_id);
    nft.initialize(&minter);

    // Set score to u32::MAX directly.
    env.as_contract(&nft_id, || {
        env.storage()
            .persistent()
            .set(&RepKey::Score(holder.clone()), &u32::MAX);
    });

    // mint should not panic; score stays at u32::MAX.
    nft.mint(&holder);
    assert_eq!(nft.balance(&holder), u32::MAX);
}

// ── Issue #2143: timestamp overflow ──────────────────────────────────────────

#[test]
fn propose_governance_action_overflow_returns_error() {
    use crate::types::{GovernanceAction, GovernanceQueueConfig};

    let env = Env::default();
    let (contract_id, _token, admin) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let admin_signers = Vec::from_array(&env, [admin.clone()]);

    // Set a timelock_delay that would overflow when added to now.
    let queue_cfg = GovernanceQueueConfig {
        timelock_delay: u64::MAX,
        execution_window: 3600,
        require_multisig: false,
    };
    client.set_governance_queue_config(&admin_signers, &queue_cfg);

    env.ledger().set_timestamp(1);

    let result = client.try_propose_governance_action(
        &admin,
        &GovernanceAction::Pause,
        &String::from_str(&env, "overflow test"),
    );
    assert_eq!(result, Err(Ok(ContractError::ArithmeticOverflow)));
}

#[test]
fn create_cross_chain_proposal_overflow_returns_error() {
    use soroban_sdk::Bytes;

    let env = Env::default();
    let (contract_id, _token, admin) = setup(&env);
    let admin_signers = Vec::from_array(&env, [admin.clone()]);

    // voting_period_seconds near u64::MAX overflows when added to now.
    env.ledger().set_timestamp(1);

    let result = env.as_contract(&contract_id, || {
        crate::cross_chain_governance::create_cross_chain_proposal(
            env.clone(),
            admin_signers.clone(),
            String::from_str(&env, "overflow"),
            String::from_str(&env, "action"),
            Bytes::new(&env),
            1u32,
            u64::MAX - 1,
        )
    });
    assert_eq!(result, Err(ContractError::ArithmeticOverflow));
}

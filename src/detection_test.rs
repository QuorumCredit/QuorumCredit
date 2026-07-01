use crate::{
    FraudFactors, FraudScoreConfig, QuorumCreditContract, QuorumCreditContractClient,
    VoucherFraudScore, DEFAULT_FRAUD_FACTORS, DEFAULT_FRAUD_SCORE_CONFIG,
};
use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

fn setup() -> (Env, QuorumCreditContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let deployer = Address::generate(&env);
    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let admins = Vec::from_array(&env, [admin1.clone(), admin2.clone()]);
    let token = env
        .register_stellar_asset_contract_v2(admin1.clone())
        .address();

    let contract_id = env.register_contract(None, QuorumCreditContract);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    client.initialize(&deployer, &admins, &2, &token);

    (env, client, admin1, admin2)
}

#[test]
fn test_update_fraud_score_new_voucher() {
    let (env, client, _admin1, _admin2) = setup();
    let voucher = Address::generate(&env);

    client.update_fraud_score(&voucher).unwrap();

    let score: VoucherFraudScore = client
        .get_fraud_score(&voucher)
        .unwrap()
        .try_into()
        .unwrap();

    assert_eq!(score.score, 0);
    assert_eq!(score.total_borrowers, 0);
    assert_eq!(score.defaulted_count, 0);
    assert_eq!(score.churn_count, 0);
    assert_eq!(score.dispute_count, 0);
}

#[test]
fn test_get_fraud_score_not_found() {
    let (_env, client, _admin1, _admin2) = setup();
    let voucher = Address::generate(&_env);

    let score = client.get_fraud_score(&voucher);
    assert!(score.is_none());
}

#[test]
fn test_fraud_score_increases_with_vouch_activity() {
    let (env, client, _admin1, _admin2) = setup();
    let voucher = Address::generate(&env);
    let borrower1 = Address::generate(&env);
    let borrower2 = Address::generate(&env);
    let borrower3 = Address::generate(&env);
    let token = client.get_token();

    client.vouch(&voucher, &borrower1, &100_000_000, &token, &None);
    client.vouch(&voucher, &borrower2, &100_000_000, &token, &None);
    client.vouch(&voucher, &borrower3, &100_000_000, &token, &None);

    client.update_fraud_score(&voucher).unwrap();

    let score: VoucherFraudScore = client
        .get_fraud_score(&voucher)
        .unwrap()
        .try_into()
        .unwrap();

    assert_eq!(score.total_borrowers, 3);
    assert_eq!(score.rapidity_score, 150);
    assert_eq!(score.default_correlation_score, 0);
}

#[test]
fn test_fraud_score_default_correlation() {
    let (env, client, admin1, _admin2) = setup();
    let voucher = Address::generate(&env);
    let borrower = Address::generate(&env);
    let token = client.get_token();

    client.vouch(&voucher, &borrower, &100_000_000, &token, &None);

    let cfg = client.get_config();
    let deadline = cfg.loan_duration;
    client.request_loan(
        &borrower,
        &100_000_000,
        &100_000_000,
        &soroban_sdk::String::from_str(&env, "test"),
        &token,
    );

    env.ledger().set_timestamp(env.ledger().timestamp() + deadline + 1);
    client.slash(&Vec::from_array(&env, [admin1.clone()]), &borrower);

    client.update_fraud_score(&voucher).unwrap();

    let score: VoucherFraudScore = client
        .get_fraud_score(&voucher)
        .unwrap()
        .try_into()
        .unwrap();

    assert_eq!(score.defaulted_count, 1);
    assert_eq!(score.default_correlation_score, 1000);
}

#[test]
fn test_fraud_score_churn_detection() {
    let (env, client, _admin1, _admin2) = setup();
    let voucher = Address::generate(&env);
    let borrower = Address::generate(&env);
    let borrower2 = Address::generate(&env);
    let token = client.get_token();

    client.vouch(&voucher, &borrower, &100_000_000, &token, &None);
    client.vouch(&voucher, &borrower2, &100_000_000, &token, &None);

    client.withdraw_vouch(&voucher, &borrower);

    client.update_fraud_score(&voucher).unwrap();

    let score: VoucherFraudScore = client
        .get_fraud_score(&voucher)
        .unwrap()
        .try_into()
        .unwrap();

    assert_eq!(score.churn_count, 1);
    assert_eq!(score.churn_score, 500);
}

#[test]
fn test_set_fraud_score_config() {
    let (env, client, admin1, admin2) = setup();

    let new_config = FraudScoreConfig {
        enabled: true,
        factors: FraudFactors {
            rapidity_weight: 2500,
            default_correlation_weight: 2500,
            churn_weight: 2500,
            dispute_weight: 2500,
        },
        high_risk_threshold: 800,
        medium_risk_threshold: 500,
    };

    client.set_fraud_score_config(&Vec::from_array(&env, [admin1, admin2]), &new_config);

    let stored_config = client.get_fraud_score_config_view();
    assert_eq!(stored_config.high_risk_threshold, 800);
    assert_eq!(stored_config.medium_risk_threshold, 500);
    assert_eq!(stored_config.factors.rapidity_weight, 2500);
}

#[test]
fn test_default_fraud_score_config() {
    let (_env, client, _admin1, _admin2) = setup();

    let config = client.get_fraud_score_config_view();
    assert_eq!(config.enabled, DEFAULT_FRAUD_SCORE_CONFIG.enabled);
    assert_eq!(
        config.high_risk_threshold,
        DEFAULT_FRAUD_SCORE_CONFIG.high_risk_threshold
    );
    assert_eq!(
        config.medium_risk_threshold,
        DEFAULT_FRAUD_SCORE_CONFIG.medium_risk_threshold
    );
    assert_eq!(
        config.factors.rapidity_weight,
        DEFAULT_FRAUD_FACTORS.rapidity_weight
    );
    assert_eq!(
        config.factors.default_correlation_weight,
        DEFAULT_FRAUD_FACTORS.default_correlation_weight
    );
    assert_eq!(
        config.factors.churn_weight,
        DEFAULT_FRAUD_FACTORS.churn_weight
    );
    assert_eq!(
        config.factors.dispute_weight,
        DEFAULT_FRAUD_FACTORS.dispute_weight
    );
}

#[test]
fn test_fraud_score_rapidity_scale() {
    let (env, client, _admin1, _admin2) = setup();
    let voucher = Address::generate(&env);
    let token = client.get_token();

    for _i in 0..20 {
        let borrower = Address::generate(&env);
        client.vouch(&voucher, &borrower, &100_000_000, &token, &None);
    }

    client.update_fraud_score(&voucher).unwrap();

    let score: VoucherFraudScore = client
        .get_fraud_score(&voucher)
        .unwrap()
        .try_into()
        .unwrap();

    assert_eq!(score.rapidity_score, 1000);
    assert_eq!(score.total_borrowers, 20);
}

#[test]
fn test_fraud_score_low_rapidity() {
    let (env, client, _admin1, _admin2) = setup();
    let voucher = Address::generate(&env);
    let token = client.get_token();

    let borrower = Address::generate(&env);
    client.vouch(&voucher, &borrower, &100_000_000, &token, &None);

    client.update_fraud_score(&voucher).unwrap();

    let score: VoucherFraudScore = client
        .get_fraud_score(&voucher)
        .unwrap()
        .try_into()
        .unwrap();

    assert_eq!(score.rapidity_score, 50);
}

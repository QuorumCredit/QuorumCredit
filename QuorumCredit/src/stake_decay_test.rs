#[cfg(test)]
mod tests {
    use crate::types::Config;
    use crate::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, Env,
    };

    fn setup() -> (Env, Address, Address, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(admin.clone()).address();
        let contract_id = env.register(QuorumCreditContract, ());
        QuorumCreditContractClient::new(&env, &contract_id).initialize(
            &deployer,
            &soroban_sdk::vec![&env, admin.clone()],
            &1u32,
            &token,
        );
        (env, contract_id, admin, token, deployer)
    }

    fn advance_time(env: &Env, secs: u64) {
        env.ledger().set(soroban_sdk::testutils::LedgerInfo {
            timestamp: env.ledger().timestamp() + secs,
            ..env.ledger().get()
        });
    }

    fn enable_decay(env: &Env, contract_id: &Address, admin: &Address, token: &Address, rate_bps: u32, period_secs: u64) {
        let client = QuorumCreditContractClient::new(env, contract_id);
        let mut cfg = client.get_config();
        cfg.decay_rate_bps = rate_bps;
        cfg.decay_period_secs = period_secs;
        client.set_config(&soroban_sdk::vec![env, admin.clone()], &cfg);
    }

    // ── compute_decayed_stake unit tests ──────────────────────────────────────

    #[test]
    fn test_no_decay_when_disabled() {
        assert_eq!(
            helpers::compute_decayed_stake(1_000_000, 0, 1_000_000, 0, 86400),
            1_000_000
        );
        assert_eq!(
            helpers::compute_decayed_stake(1_000_000, 0, 1_000_000, 100, 0),
            1_000_000
        );
    }

    #[test]
    fn test_no_decay_before_first_period() {
        // 100 bps = 1% per 30 days; only 15 days elapsed → 0 full periods
        let stake = 1_000_000i128;
        let now = 15 * 86400u64;
        let result = helpers::compute_decayed_stake(stake, 0, now, 100, 30 * 86400);
        assert_eq!(result, stake);
    }

    #[test]
    fn test_one_period_decay() {
        // 1% decay after 1 period: 1_000_000 * 9900 / 10_000 = 990_000
        let stake = 1_000_000i128;
        let period = 30 * 86400u64;
        let result = helpers::compute_decayed_stake(stake, 0, period, 100, period);
        assert_eq!(result, 990_000);
    }

    #[test]
    fn test_two_period_decay() {
        // After 2 periods: 990_000 * 9900 / 10_000 = 980_100
        let stake = 1_000_000i128;
        let period = 30 * 86400u64;
        let result = helpers::compute_decayed_stake(stake, 0, 2 * period, 100, period);
        assert_eq!(result, 980_100);
    }

    #[test]
    fn test_decay_never_negative() {
        // Very high decay rate over many periods should floor at 0
        let stake = 1_000i128;
        let period = 1u64;
        // 9999 bps = 99.99% decay per second — after enough seconds → 0
        let result = helpers::compute_decayed_stake(stake, 0, 100, 9999, period);
        assert_eq!(result, 0);
    }

    // ── Integration: total_vouched applies decay ──────────────────────────────

    #[test]
    fn test_total_vouched_no_decay_by_default() {
        let (env, contract_id, admin, token, _) = setup();
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        let voucher = Address::generate(&env);
        let borrower = Address::generate(&env);
        StellarAssetClient::new(&env, &token).mint(&voucher, &10_000_000);

        client.vouch(&voucher, &borrower, &1_000_000, &token);
        advance_time(&env, 90 * 86400); // 90 days — no decay configured

        assert_eq!(client.total_vouched(&borrower).unwrap(), 1_000_000);
    }

    #[test]
    fn test_total_vouched_decays_after_period() {
        let (env, contract_id, admin, token, _) = setup();
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        let voucher = Address::generate(&env);
        let borrower = Address::generate(&env);
        StellarAssetClient::new(&env, &token).mint(&voucher, &10_000_000);

        client.vouch(&voucher, &borrower, &1_000_000, &token);

        // Enable 1% decay per 30 days
        enable_decay(&env, &contract_id, &admin, &token, 100, 30 * 86400);

        advance_time(&env, 30 * 86400 + 1); // just past 1 period

        // 1_000_000 * 9900 / 10_000 = 990_000
        assert_eq!(client.total_vouched(&borrower).unwrap(), 990_000);
    }

    // ── Integration: is_eligible applies decay ────────────────────────────────

    #[test]
    fn test_is_eligible_false_after_decay_drops_below_threshold() {
        let (env, contract_id, admin, token, _) = setup();
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        let voucher = Address::generate(&env);
        let borrower = Address::generate(&env);
        StellarAssetClient::new(&env, &token).mint(&voucher, &10_000_000);

        // Stake exactly at threshold
        client.vouch(&voucher, &borrower, &1_000_000, &token);

        // Enable 5% decay per 30 days
        enable_decay(&env, &contract_id, &admin, &token, 500, 30 * 86400);

        // Before decay: eligible at threshold 1_000_000
        assert!(client.is_eligible(&borrower, &1_000_000));

        advance_time(&env, 30 * 86400 + 1); // 1 period → 950_000

        // After decay: 950_000 < 1_000_000 → not eligible
        assert!(!client.is_eligible(&borrower, &1_000_000));
    }

    // ── refresh_vouch resets decay clock ─────────────────────────────────────

    #[test]
    fn test_refresh_vouch_resets_decay() {
        let (env, contract_id, admin, token, _) = setup();
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        let voucher = Address::generate(&env);
        let borrower = Address::generate(&env);
        StellarAssetClient::new(&env, &token).mint(&voucher, &10_000_000);

        client.vouch(&voucher, &borrower, &1_000_000, &token);
        enable_decay(&env, &contract_id, &admin, &token, 100, 30 * 86400);

        advance_time(&env, 30 * 86400 + 1); // 1 period elapsed → 990_000

        // Refresh resets the timestamp
        client.refresh_vouch(&voucher, &borrower);

        // Immediately after refresh: no full period has elapsed → full stake
        assert_eq!(client.total_vouched(&borrower).unwrap(), 1_000_000);
    }

    #[test]
    fn test_refresh_vouch_nonexistent_fails() {
        let (env, contract_id, _, _, _) = setup();
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        let voucher = Address::generate(&env);
        let borrower = Address::generate(&env);

        let result = client.try_refresh_vouch(&voucher, &borrower);
        assert!(result.is_err());
    }
}

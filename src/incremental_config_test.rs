/// Issue #938: Incremental Config Changes
/// Tests for enqueue_config_patch / apply_next_config_patch
#[cfg(test)]
mod incremental_config_tests {
    use crate::{ConfigField, QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, Env, Vec,
    };

    struct Setup {
        env: Env,
        client: QuorumCreditContractClient<'static>,
        admin: Address,
        token: Address,
    }

    fn setup() -> Setup {
        let env = Env::default();
        env.mock_all_auths();
        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let admins = Vec::from_array(&env, [admin.clone()]);
        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        let contract_id = env.register_contract(None, QuorumCreditContract);
        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &10_000_000_000);
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token_id.address());
        env.ledger().with_mut(|l| l.timestamp = 1_000);
        Setup { env, client, admin, token: token_id.address() }
    }

    fn admins(s: &Setup) -> Vec<Address> {
        Vec::from_array(&s.env, [s.admin.clone()])
    }

    #[test]
    fn test_enqueue_and_apply_yield_bps_patch() {
        let s = setup();

        // Enqueue a patch to change yield_bps to 300 (3%), applicable immediately.
        s.client.enqueue_config_patch(&admins(&s), &ConfigField::YieldBps, &300, &1_000);

        assert_eq!(s.client.get_config_patch_count(), 1);

        let patch = s.client.get_config_patch(&0).unwrap();
        assert!(!patch.applied);

        // Apply the patch — should succeed and return true.
        let applied = s.client.apply_next_config_patch();
        assert!(applied);

        // Config should now reflect the new yield_bps.
        assert_eq!(s.client.get_config().yield_bps, 300);

        // The patch is now marked applied.
        let patch_after = s.client.get_config_patch(&0).unwrap();
        assert!(patch_after.applied);

        // Calling again with no more pending patches returns false.
        let applied_again = s.client.apply_next_config_patch();
        assert!(!applied_again);
    }

    #[test]
    fn test_patch_not_applied_before_apply_after() {
        let s = setup();

        // Enqueue a patch with apply_after in the future.
        let future_ts = 1_000 + 7 * 24 * 60 * 60; // 7 days from now
        s.client.enqueue_config_patch(&admins(&s), &ConfigField::SlashBps, &3_000, &future_ts);

        // Trying to apply now should return false (not yet eligible).
        let applied = s.client.apply_next_config_patch();
        assert!(!applied);

        // Config slash_bps unchanged.
        assert_eq!(s.client.get_config().slash_bps, 5_000); // default

        // Advance time past apply_after.
        s.env.ledger().with_mut(|l| l.timestamp = future_ts + 1);

        // Now the patch should apply.
        let applied_late = s.client.apply_next_config_patch();
        assert!(applied_late);
        assert_eq!(s.client.get_config().slash_bps, 3_000);
    }

    #[test]
    fn test_multiple_patches_applied_in_order() {
        let s = setup();

        s.client.enqueue_config_patch(&admins(&s), &ConfigField::YieldBps, &150, &1_000);
        s.client.enqueue_config_patch(&admins(&s), &ConfigField::MaxVouchers, &20, &1_000);

        assert_eq!(s.client.get_config_patch_count(), 2);

        // First call applies patch 0 (YieldBps).
        s.client.apply_next_config_patch();
        assert_eq!(s.client.get_config().yield_bps, 150);

        // Second call applies patch 1 (MaxVouchers).
        s.client.apply_next_config_patch();
        assert_eq!(s.client.get_config().max_vouchers, 20);
    }

    #[test]
    fn test_patch_min_loan_amount() {
        let s = setup();
        s.client.enqueue_config_patch(&admins(&s), &ConfigField::MinLoanAmount, &200_000, &1_000);
        s.client.apply_next_config_patch();
        assert_eq!(s.client.get_config().min_loan_amount, 200_000);
    }
}

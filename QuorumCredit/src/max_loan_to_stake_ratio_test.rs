/// Tests verifying that set_max_loan_to_stake_ratio rejects a zero ratio.
#[cfg(test)]
mod max_loan_to_stake_ratio_tests {
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

    fn setup() -> (Env, QuorumCreditContractClient<'static>, Address) {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let admins = Vec::from_array(&env, [admin.clone()]);
        let token = env.register_stellar_asset_contract_v2(admin.clone());
        let contract_id = env.register_contract(None, QuorumCreditContract);

        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token.address());

        (env, client, admin)
    }

    #[test]
    #[should_panic]
    fn test_set_max_loan_to_stake_ratio_rejects_zero() {
        let (env, client, admin) = setup();
        let admin_signers = Vec::from_array(&env, [admin]);
        client.set_max_loan_to_stake_ratio(&admin_signers, &0);
    }

    #[test]
    fn test_set_max_loan_to_stake_ratio_accepts_positive() {
        let (env, client, admin) = setup();
        let admin_signers = Vec::from_array(&env, [admin]);
        client.set_max_loan_to_stake_ratio(&admin_signers, &200);
        assert_eq!(client.get_config().max_loan_to_stake_ratio, 200);
    }
}

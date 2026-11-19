//! Tests for issue #2071: claiming an SBT stake before its unlock time must
//! return the dedicated `ContractError::StakeNotUnlocked`, not
//! `LoanPastDeadline`.
#[cfg(test)]
mod sbt_stake_not_unlocked_tests {
    use crate::errors::ContractError;
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, Env, Vec,
    };

    #[test]
    fn early_sbt_claim_returns_stake_not_unlocked() {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let admins = Vec::from_array(&env, [admin.clone()]);
        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        let contract_id = env.register_contract(None, QuorumCreditContract);
        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &1_000_000_000);
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token_id.address());
        env.ledger().with_mut(|l| l.timestamp = 120);

        let holder = Address::generate(&env);
        StellarAssetClient::new(&env, &token_id.address()).mint(&holder, &1_000_000);

        let sbt_id: u64 = 1;
        let duration: u64 = 30 * 24 * 60 * 60;
        env.as_contract(&contract_id, || {
            crate::sbt_staking::stake_sbt(&env, holder.clone(), sbt_id, 100_000, duration)
                .unwrap();
            // Claim immediately, well before `unlocks_at`.
            let result = crate::sbt_staking::claim_sbt_reward(&env, holder.clone(), sbt_id);
            assert_eq!(result, Err(ContractError::StakeNotUnlocked));
        });
    }

    #[test]
    fn stake_not_unlocked_error_code_is_stable_and_distinct() {
        assert_eq!(ContractError::StakeNotUnlocked as u32, 266);
        assert_ne!(
            ContractError::StakeNotUnlocked as u32,
            ContractError::LoanPastDeadline as u32
        );
    }
}

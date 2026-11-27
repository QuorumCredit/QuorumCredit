//! Tests for issue #2165: SBT Marketplace Listing Does Not Verify That the Caller Owns the SBT
#[cfg(test)]
mod sbt_marketplace_ownership_tests {
    use crate::errors::ContractError;
    use crate::types::DataKey;
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, Bytes, Env, Vec,
    };

    #[test]
    fn non_owner_cannot_list_sbt_in_marketplace() {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let admins = Vec::from_array(&env, [admin.clone()]);
        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token_id.address());

        let owner = Address::generate(&env);
        let non_owner = Address::generate(&env);
        let sbt_id: u64 = 42;

        env.as_contract(&contract_id, || {
            // Establish ownership via privacy record
            crate::sbt_privacy_modes::set_sbt_privacy_mode(
                &env,
                owner.clone(),
                sbt_id,
                crate::sbt_privacy_modes::PrivacyMode::Public,
            )
            .unwrap();

            let metadata = Bytes::from_slice(&env, b"credential metadata");
            let tag = Bytes::from_slice(&env, b"identity");

            // Attempt listing by non-owner must fail with UnauthorizedCaller
            let res = crate::sbt_marketplace::list_sbt_in_marketplace(
                &env,
                non_owner.clone(),
                sbt_id,
                metadata.clone(),
                tag.clone(),
            );
            assert_eq!(res, Err(ContractError::UnauthorizedCaller));

            // Owner listing must succeed
            let res_owner = crate::sbt_marketplace::list_sbt_in_marketplace(
                &env,
                owner.clone(),
                sbt_id,
                metadata,
                tag,
            );
            assert!(res_owner.is_ok());
        });
    }
}

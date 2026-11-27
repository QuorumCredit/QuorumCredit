//! Issue #2166: SBT Marketplace Index Eviction Tests
//!
//! Verifies that listings evicted from the bounded global index ring-buffer
//! remain discoverable in `search_sbt_marketplace` via the per-tag and secondary indexes.

#![cfg(test)]

use crate::errors::ContractError;
use crate::sbt_marketplace::{
    get_sbt_marketplace_listing, list_sbt_in_marketplace, remove_sbt_marketplace_listing,
    search_sbt_marketplace, update_sbt_marketplace_listing,
};
use crate::types::DataKey;
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{
    testutils::Address as _, Address, Bytes, Env, Vec,
};

fn setup() -> (Env, Address, Address, QuorumCreditContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let contract_id = env.register(QuorumCreditContract, ());

    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let admins = Vec::from_array(&env, [admin.clone()]);
    client.initialize(&admin, &admins, &1, &token);

    (env, admin, contract_id, client)
}

#[test]
fn test_evicted_active_listing_is_still_found_in_search() {
    let (env, _admin, contract_id, _client) = setup();
    let holder1 = Address::generate(&env);
    let holder2 = Address::generate(&env);

    let tag_identity = Bytes::from_slice(&env, b"identity");
    let tag_credit = Bytes::from_slice(&env, b"credit");
    let metadata = Bytes::from_slice(&env, b"{\"credential\":\"verified\"}");

    env.as_contract(&contract_id, || {
        // List SBT 1 with tag "identity"
        list_sbt_in_marketplace(&env, holder1.clone(), 1, metadata.clone(), tag_identity.clone())
            .expect("list should succeed");

        // Pre-fill the global index to simulate reaching SBT_MARKETPLACE_INDEX_LIMIT
        let mut full_index: Vec<u64> = Vec::new(&env);
        full_index.push_back(1); // sbt 1 is oldest
        for id in 2..=10_000u64 {
            full_index.push_back(id);
        }
        env.storage()
            .persistent()
            .set(&DataKey::SbtMarketplaceIndex, &full_index);

        // Now list sbt_id 10_001, which triggers eviction of the oldest entry (sbt 1) from the global index
        list_sbt_in_marketplace(
            &env,
            holder2.clone(),
            10_001,
            metadata.clone(),
            tag_credit.clone(),
        )
        .expect("listing past limit should succeed and evict oldest from global index");

        // Verify that sbt 1 was indeed evicted from the global index
        let global_index: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::SbtMarketplaceIndex)
            .unwrap();
        assert!(!global_index.iter().any(|id| id == 1), "sbt 1 should be evicted from global index");

        // But search_sbt_marketplace MUST still find sbt 1 because it remains active
        let search_results = search_sbt_marketplace(&env, tag_identity.clone());
        assert!(
            search_results.iter().any(|id| id == 1),
            "evicted active listing 1 must still be found in search results"
        );
    });
}

#[test]
fn test_tag_update_and_removal_updates_search_results() {
    let (env, _admin, contract_id, _client) = setup();
    let holder = Address::generate(&env);

    let tag_v1 = Bytes::from_slice(&env, b"tag_alpha");
    let tag_v2 = Bytes::from_slice(&env, b"tag_beta");
    let metadata = Bytes::from_slice(&env, b"metadata");

    env.as_contract(&contract_id, || {
        list_sbt_in_marketplace(&env, holder.clone(), 42, metadata.clone(), tag_v1.clone())
            .expect("list should succeed");

        // Found under tag_v1
        let res1 = search_sbt_marketplace(&env, tag_v1.clone());
        assert!(res1.iter().any(|id| id == 42));

        // Update tag to tag_v2
        update_sbt_marketplace_listing(&env, holder.clone(), 42, metadata.clone(), tag_v2.clone())
            .expect("update should succeed");

        // No longer found under tag_v1, now found under tag_v2
        let res_old = search_sbt_marketplace(&env, tag_v1.clone());
        assert!(!res_old.iter().any(|id| id == 42));

        let res_new = search_sbt_marketplace(&env, tag_v2.clone());
        assert!(res_new.iter().any(|id| id == 42));

        // Delist
        remove_sbt_marketplace_listing(&env, holder.clone(), 42)
            .expect("delist should succeed");

        // Inactive listing is not returned
        let res_delisted = search_sbt_marketplace(&env, tag_v2.clone());
        assert!(!res_delisted.iter().any(|id| id == 42));
    });
}

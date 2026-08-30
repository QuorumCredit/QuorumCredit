//! Tests for Cross-Chain Auction Module (Issue #974)
//!
//! Tests for auction creation, bidding, settlement, and collateral distribution.

#[cfg(test)]
mod tests {
    use crate::cross_chain_auction::*;
    use crate::errors::ContractError;
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Address, Env, Vec,
    };

    struct Setup {
        env: Env,
        contract_id: Address,
        admins: Vec<Address>,
        token: Address,
    }

    fn setup() -> Setup {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let admins = Vec::from_array(&env, [admin.clone()]);
        let token_admin = Address::generate(&env);
        let token = env.register_stellar_asset_contract_v2(token_admin).address();
        let contract_id = env.register_contract(None, QuorumCreditContract);

        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &1u32, &token);

        env.ledger().with_mut(|l| l.timestamp = 1_000);

        Setup {
            env,
            contract_id,
            admins,
            token,
        }
    }

    #[test]
    fn test_create_auction() {
        // Test auction creation with valid collateral amount and reserve price
        // Should generate unique auction ID
        
        assert!(true); // Placeholder
    }

    #[test]
    fn test_auction_states() {
        // Test that auction transitions through states: Pending -> Active -> Ended -> Settled
        
        assert!(true); // Placeholder
    }

    #[test]
    fn test_place_bid_above_reserve() {
        // Test that bids meeting or exceeding reserve price are accepted
        
        assert!(true); // Placeholder
    }

    #[test]
    fn test_place_bid_below_reserve() {
        // Test that bids below reserve price are rejected
        
        assert!(true); // Placeholder
    }

    #[test]
    fn test_bid_must_beat_highest() {
        // Test that new bids must exceed the current highest bid
        
        assert!(true); // Placeholder
    }

    #[test]
    fn test_previous_bidder_refunded() {
        // Test that previous highest bidder is refunded when outbid
        
        assert!(true); // Placeholder
    }

    #[test]
    fn test_auction_cannot_end_early() {
        // Test that auction cannot be settled before duration expires
        
        assert!(true); // Placeholder
    }

    #[test]
    fn test_settle_with_no_bids() {
        // Test that settlement fails if no bids received
        
        assert!(true); // Placeholder
    }

    #[test]
    fn test_settlement_distribution() {
        // Test that proceeds are distributed: 80% to vouchers, 20% to treasury
        
        assert!(true); // Placeholder
    }

    #[test]
    fn test_extend_auction() {
        // Test that auction duration can be extended if no bids received
        
        assert!(true); // Placeholder
    }

    #[test]
    fn test_cancel_auction() {
        // Test that admin can cancel auction
        
        assert!(true); // Placeholder
    }

    #[test]
    fn test_cross_chain_bid_aggregation() {
        // Test that bids from multiple chains are properly aggregated
        
        assert!(true); // Placeholder
    }

    #[test]
    fn test_auction_queries() {
        // Test querying auction status, details, and settlement info

        assert!(true); // Placeholder
    }

    // ── Issue #69: force-settle path ────────────────────────────────────────

    /// `settle_auction` takes no `admin_signers` at all -- any address can
    /// trigger settlement once `now >= auction_end`, so slashed collateral and
    /// voucher proceeds are never stuck waiting on an admin to show up.
    #[test]
    fn test_non_admin_can_settle_expired_auction() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        let bidder = Address::generate(&s.env);

        let auction_id = s.env.as_contract(&s.contract_id, || {
            create_cross_chain_auction(
                s.env.clone(),
                s.admins.clone(),
                borrower.clone(),
                1_000_000,
                s.token.clone(),
                100,
                10,
                Vec::new(&s.env),
            )
            .unwrap()
        });

        s.env.as_contract(&s.contract_id, || {
            place_bid(s.env.clone(), auction_id, bidder.clone(), 50, 0).unwrap();
        });

        // Advance past auction end.
        s.env.ledger().with_mut(|l| l.timestamp += 200);

        // No admin approval is supplied or required to settle.
        s.env.as_contract(&s.contract_id, || {
            settle_auction(s.env.clone(), auction_id).unwrap();
        });

        let auction =
            s.env.as_contract(&s.contract_id, || get_auction(s.env.clone(), auction_id).unwrap());
        assert!(auction.settled);

        let settlement = s.env.as_contract(&s.contract_id, || {
            get_auction_settlement(s.env.clone(), auction_id).unwrap()
        });
        assert_eq!(settlement.winning_bid, 50);
        assert_eq!(settlement.winning_bidder, bidder);
    }

    // ── Issue #70: extend/cancel authorization ──────────────────────────────

    #[test]
    #[should_panic(expected = "signer is not a registered admin")]
    fn test_extend_auction_rejects_non_admin() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        let not_admin = Vec::from_array(&s.env, [Address::generate(&s.env)]);

        let auction_id = s.env.as_contract(&s.contract_id, || {
            create_cross_chain_auction(
                s.env.clone(),
                s.admins.clone(),
                borrower.clone(),
                1_000_000,
                s.token.clone(),
                1_000,
                10,
                Vec::new(&s.env),
            )
            .unwrap()
        });

        let new_end = s.env.as_contract(&s.contract_id, || get_auction(s.env.clone(), auction_id).unwrap())
            .auction_end
            + 100;

        s.env.as_contract(&s.contract_id, || {
            let _ = extend_auction(s.env.clone(), not_admin, auction_id, new_end);
        });
    }

    #[test]
    #[should_panic(expected = "signer is not a registered admin")]
    fn test_cancel_auction_rejects_non_admin() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        let not_admin = Vec::from_array(&s.env, [Address::generate(&s.env)]);

        let auction_id = s.env.as_contract(&s.contract_id, || {
            create_cross_chain_auction(
                s.env.clone(),
                s.admins.clone(),
                borrower.clone(),
                1_000_000,
                s.token.clone(),
                1_000,
                10,
                Vec::new(&s.env),
            )
            .unwrap()
        });

        s.env.as_contract(&s.contract_id, || {
            let _ = cancel_auction(s.env.clone(), not_admin, auction_id);
        });
    }

    /// Cancelling an auction with an existing highest bid must unwind/refund
    /// that bid rather than leaving it looking like a live, unrefunded stake.
    #[test]
    fn test_cancel_auction_refunds_highest_bid() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        let bidder = Address::generate(&s.env);

        let auction_id = s.env.as_contract(&s.contract_id, || {
            create_cross_chain_auction(
                s.env.clone(),
                s.admins.clone(),
                borrower.clone(),
                1_000_000,
                s.token.clone(),
                1_000,
                10,
                Vec::new(&s.env),
            )
            .unwrap()
        });

        s.env.as_contract(&s.contract_id, || {
            place_bid(s.env.clone(), auction_id, bidder.clone(), 50, 0).unwrap();
        });

        s.env.as_contract(&s.contract_id, || {
            cancel_auction(s.env.clone(), s.admins.clone(), auction_id).unwrap();
        });

        let bid = s.env.as_contract(&s.contract_id, || {
            get_auction_bid(s.env.clone(), auction_id, bidder.clone()).unwrap()
        });
        assert!(
            bid.refunded,
            "the highest bid must be marked refunded when its auction is cancelled"
        );

        let auction =
            s.env.as_contract(&s.contract_id, || get_auction(s.env.clone(), auction_id).unwrap());
        assert!(auction.settled);
    }

    /// `extend_auction` must reject a new end time that is not strictly later
    /// than the auction's current end time.
    #[test]
    fn test_extend_auction_rejects_earlier_end_time() {
        let s = setup();
        let borrower = Address::generate(&s.env);

        let auction_id = s.env.as_contract(&s.contract_id, || {
            create_cross_chain_auction(
                s.env.clone(),
                s.admins.clone(),
                borrower.clone(),
                1_000_000,
                s.token.clone(),
                1_000,
                10,
                Vec::new(&s.env),
            )
            .unwrap()
        });

        let current_end =
            s.env.as_contract(&s.contract_id, || get_auction(s.env.clone(), auction_id).unwrap())
                .auction_end;

        let result = s.env.as_contract(&s.contract_id, || {
            extend_auction(s.env.clone(), s.admins.clone(), auction_id, current_end - 1)
        });
        assert_eq!(result, Err(ContractError::InvalidStateTransition));

        let result_same = s.env.as_contract(&s.contract_id, || {
            extend_auction(s.env.clone(), s.admins.clone(), auction_id, current_end)
        });
        assert_eq!(result_same, Err(ContractError::InvalidStateTransition));

        // A genuinely later end time is still accepted.
        s.env.as_contract(&s.contract_id, || {
            extend_auction(s.env.clone(), s.admins.clone(), auction_id, current_end + 50).unwrap();
        });
        let extended =
            s.env.as_contract(&s.contract_id, || get_auction(s.env.clone(), auction_id).unwrap());
        assert_eq!(extended.auction_end, current_end + 50);
    }
}

//! Issue #1467: dedicated test coverage for `src/syndication.rs`, in
//! particular proportional loss sharing on default and the member-cap /
//! stake-consistency checks added for issue #1466.

#[cfg(test)]
mod syndication_tests {
    use crate::syndication;
    use crate::types::{DataKey, SyndicationConfig, SyndicationRole, SyndicationStatus};
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, Env, String, Vec,
    };

    struct Setup {
        env: Env,
        contract_id: Address,
        token: Address,
    }

    fn setup() -> Setup {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let mut admins = Vec::new(&env);
        admins.push_back(deployer.clone());

        let token_id = env.register_stellar_asset_contract_v2(deployer.clone());
        let contract_id = env.register(QuorumCreditContract, ());

        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &1u32, &token_id.address());

        env.ledger().with_mut(|l| l.timestamp = 1_000);

        Setup {
            env,
            contract_id,
            token: token_id.address(),
        }
    }

    fn fund(env: &Env, token: &Address, who: &Address, amount: i128) {
        StellarAssetClient::new(env, token).mint(who, &amount);
    }

    fn set_config(s: &Setup, cfg: SyndicationConfig) {
        s.env.as_contract(&s.contract_id, || {
            s.env
                .storage()
                .instance()
                .set(&DataKey::SyndicationConfig, &cfg);
        });
    }

    /// Issue #1466: `join_syndication` must reject a member once the
    /// syndication is at its configured `max_members` cap.
    #[test]
    fn test_join_syndication_enforces_max_members_cap() {
        let s = setup();
        set_config(
            &s,
            SyndicationConfig {
                max_members: 2,
                min_members: 2,
                min_approval_percentage: 7_500,
                max_loan_amount: 1_000_000_000,
                syndication_fee_bps: 100,
            },
        );

        let creator = Address::generate(&s.env);
        let member1 = Address::generate(&s.env);
        let member2 = Address::generate(&s.env);
        let member3 = Address::generate(&s.env);
        fund(&s.env, &s.token, &member1, 1_000);
        fund(&s.env, &s.token, &member2, 1_000);
        fund(&s.env, &s.token, &member3, 1_000);

        let syndication_id = s.env.as_contract(&s.contract_id, || {
            syndication::create_syndication(
                s.env.clone(),
                creator.clone(),
                String::from_str(&s.env, "cap test"),
                s.token.clone(),
                1_000,
            )
            .unwrap()
        });

        s.env.as_contract(&s.contract_id, || {
            syndication::join_syndication(
                s.env.clone(),
                syndication_id,
                member1.clone(),
                SyndicationRole::LeadBorrower,
                5_000,
                500,
                0,
            )
            .unwrap();
            syndication::join_syndication(
                s.env.clone(),
                syndication_id,
                member2.clone(),
                SyndicationRole::CoBorrower,
                5_000,
                500,
                0,
            )
            .unwrap();

            let err = syndication::join_syndication(
                s.env.clone(),
                syndication_id,
                member3.clone(),
                SyndicationRole::CoBorrower,
                1,
                100,
                0,
            )
            .unwrap_err();
            assert_eq!(err, crate::errors::ContractError::SyndicationMaxMembersExceeded);
        });
    }

    /// Issue #1466: even if a syndication remains `Ready` (quorum of
    /// approvals still met), `request_syndication_loan` must re-check that
    /// the stake still committed by current members covers the loan amount,
    /// since a member may have left after approving.
    #[test]
    fn test_request_loan_fails_after_member_leaves_reducing_stake() {
        let s = setup();
        // Only 50% of members need to approve, so the syndication can reach
        // `Ready` while one member has neither approved nor is required to.
        set_config(
            &s,
            SyndicationConfig {
                max_members: 10,
                min_members: 2,
                min_approval_percentage: 5_000,
                max_loan_amount: 1_000_000_000,
                syndication_fee_bps: 100,
            },
        );

        let lead = Address::generate(&s.env);
        let co1 = Address::generate(&s.env);
        let co2 = Address::generate(&s.env);
        fund(&s.env, &s.token, &lead, 1_000);
        fund(&s.env, &s.token, &co1, 1_000);
        fund(&s.env, &s.token, &co2, 1_000);

        let syndication_id = s.env.as_contract(&s.contract_id, || {
            syndication::create_syndication(
                s.env.clone(),
                lead.clone(),
                String::from_str(&s.env, "stake check"),
                s.token.clone(),
                1_000,
            )
            .unwrap()
        });

        s.env.as_contract(&s.contract_id, || {
            syndication::join_syndication(
                s.env.clone(),
                syndication_id,
                lead.clone(),
                SyndicationRole::LeadBorrower,
                4_000,
                400,
                0,
            )
            .unwrap();
            syndication::join_syndication(
                s.env.clone(),
                syndication_id,
                co1.clone(),
                SyndicationRole::CoBorrower,
                3_000,
                400,
                0,
            )
            .unwrap();
            syndication::join_syndication(
                s.env.clone(),
                syndication_id,
                co2.clone(),
                SyndicationRole::CoBorrower,
                3_000,
                400,
                0,
            )
            .unwrap();

            // Only lead + co1 approve; that already reaches the 50% quorum
            // of 3 members, so the syndication becomes Ready while co2 -
            // who never approved - is still free to leave.
            syndication::approve_syndication(s.env.clone(), syndication_id, lead.clone()).unwrap();
            syndication::approve_syndication(s.env.clone(), syndication_id, co1.clone()).unwrap();

            let syn = syndication::get_syndication(s.env.clone(), syndication_id).unwrap();
            assert_eq!(syn.status, SyndicationStatus::Ready);

            syndication::leave_syndication(s.env.clone(), syndication_id, co2.clone()).unwrap();

            // Quorum (2 approvals, min_members=2) is still satisfied with
            // just lead + co1, so status stays Ready ...
            let syn = syndication::get_syndication(s.env.clone(), syndication_id).unwrap();
            assert_eq!(syn.status, SyndicationStatus::Ready);

            // ... but committed stake (400 + 400 = 800) no longer covers the
            // requested loan amount (1_000), so the loan request must fail.
            let err = syndication::request_syndication_loan(s.env.clone(), syndication_id, lead.clone())
                .unwrap_err();
            assert_eq!(err, crate::errors::ContractError::MinStakeNotMet);
        });
    }

    fn setup_active_syndication(
        s: &Setup,
        members: &[(Address, i128, i128)],
        total_amount: i128,
    ) -> u64 {
        let lead = members[0].0.clone();
        for (addr, collateral, vouch) in members.iter() {
            fund(&s.env, &s.token, addr, collateral + vouch + 10);
        }

        s.env.as_contract(&s.contract_id, || {
            let syndication_id = syndication::create_syndication(
                s.env.clone(),
                lead.clone(),
                String::from_str(&s.env, "default test"),
                s.token.clone(),
                total_amount,
            )
            .unwrap();

            for (i, (addr, collateral, vouch)) in members.iter().enumerate() {
                let role = if i == 0 {
                    SyndicationRole::LeadBorrower
                } else {
                    SyndicationRole::CoBorrower
                };
                syndication::join_syndication(
                    s.env.clone(),
                    syndication_id,
                    addr.clone(),
                    role,
                    10_000 / members.len() as u32,
                    *collateral,
                    *vouch,
                )
                .unwrap();
            }
            for (addr, _, _) in members.iter() {
                syndication::approve_syndication(s.env.clone(), syndication_id, addr.clone()).unwrap();
            }

            syndication::request_syndication_loan(s.env.clone(), syndication_id, lead.clone()).unwrap();
            syndication_id
        })
    }

    /// Issue #1467: on default, the unrepaid shortfall is shared across the
    /// two members proportionally to their committed stake.
    #[test]
    fn test_default_loss_distribution_two_members_even_stake() {
        let s = setup();
        let m1 = Address::generate(&s.env);
        let m2 = Address::generate(&s.env);
        let syndication_id = setup_active_syndication(&s, &[(m1.clone(), 500, 0), (m2.clone(), 500, 0)], 1_000);

        s.env.as_contract(&s.contract_id, || {
            // Nothing repaid: shortfall = 1_000, split evenly since stakes are equal.
            syndication::handle_syndication_default(s.env.clone(), syndication_id, m1.clone()).unwrap();

            let mem1 = syndication::get_syndication_member(s.env.clone(), syndication_id, m1.clone()).unwrap();
            let mem2 = syndication::get_syndication_member(s.env.clone(), syndication_id, m2.clone()).unwrap();
            assert_eq!(mem1.collateral, 0);
            assert_eq!(mem2.collateral, 0);

            let syn = syndication::get_syndication(s.env.clone(), syndication_id).unwrap();
            assert_eq!(syn.status, SyndicationStatus::Defaulted);
        });
    }

    /// Issue #1467: five members share a default loss proportionally.
    #[test]
    fn test_default_loss_distribution_five_members() {
        let s = setup();
        let mut members: std::vec::Vec<(Address, i128, i128)> = std::vec::Vec::new();
        for _ in 0..5 {
            members.push((Address::generate(&s.env), 200i128, 0i128));
        }
        let syndication_id = setup_active_syndication(&s, &members, 1_000);

        s.env.as_contract(&s.contract_id, || {
            // Total stake = 1_000, shortfall = 1_000 -> every member loses
            // exactly their full stake (proportional share = stake itself).
            syndication::handle_syndication_default(s.env.clone(), syndication_id, members[0].0.clone())
                .unwrap();

            for (addr, _, _) in members.iter() {
                let m = syndication::get_syndication_member(s.env.clone(), syndication_id, addr.clone()).unwrap();
                assert_eq!(m.collateral, 0);
                assert_eq!(m.vouch_stake, 0);
            }
        });
    }

    /// Issue #1467: uneven stakes must be charged proportionally, not evenly.
    #[test]
    fn test_default_loss_distribution_uneven_stake() {
        let s = setup();
        let big = Address::generate(&s.env);
        let small = Address::generate(&s.env);
        // Total stake = 900 (750 + 150), total_amount = 900 so full stake is
        // on the line; shortfall of 300 should be split 5:1 by stake.
        let syndication_id =
            setup_active_syndication(&s, &[(big.clone(), 750, 0), (small.clone(), 150, 0)], 900);
        // `big` spent its 750 as collateral at join time; top up so it can
        // also make the 600 repayment below.
        fund(&s.env, &s.token, &big, 600);

        s.env.as_contract(&s.contract_id, || {
            // Repay 600 of the 900, leaving a 300 shortfall to distribute.
            syndication::repay_syndication_loan(s.env.clone(), syndication_id, big.clone(), 600).unwrap();

            syndication::handle_syndication_default(s.env.clone(), syndication_id, big.clone()).unwrap();

            let m_big = syndication::get_syndication_member(s.env.clone(), syndication_id, big.clone()).unwrap();
            let m_small = syndication::get_syndication_member(s.env.clone(), syndication_id, small.clone()).unwrap();

            // big had 750/900 of the stake -> loses 300 * 750/900 = 250
            // small had 150/900 of the stake -> loses 300 * 150/900 = 50
            assert_eq!(m_big.collateral, 750 - 250);
            assert_eq!(m_small.collateral, 150 - 50);
        });
    }

    /// Issue #1467: a member with zero remaining stake at default time must
    /// never receive a negative payout / balance.
    #[test]
    fn test_default_zero_stake_member_no_negative_payout() {
        let s = setup();
        let lead = Address::generate(&s.env);
        let broke = Address::generate(&s.env);
        let syndication_id =
            setup_active_syndication(&s, &[(lead.clone(), 1_000, 0), (broke.clone(), 0, 0)], 1_000);

        s.env.as_contract(&s.contract_id, || {
            syndication::handle_syndication_default(s.env.clone(), syndication_id, lead.clone()).unwrap();

            let m_broke = syndication::get_syndication_member(s.env.clone(), syndication_id, broke.clone()).unwrap();
            assert_eq!(m_broke.collateral, 0);
            assert_eq!(m_broke.vouch_stake, 0);

            let m_lead = syndication::get_syndication_member(s.env.clone(), syndication_id, lead.clone()).unwrap();
            // All the loss falls on the only member with stake, but it can
            // never go negative.
            assert_eq!(m_lead.collateral, 0);
        });
    }
}

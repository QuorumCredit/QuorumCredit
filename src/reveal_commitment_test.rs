#[cfg(test)]
mod reveal_commitment_tests {
    use crate::errors::ContractError;
    use crate::zk_snarks;
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, Bytes, Env, String, Vec,
    };

    struct Setup {
        env: Env,
        client: QuorumCreditContractClient<'static>,
        token: Address,
    }

    fn setup() -> Setup {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let mut admins = Vec::new(&env);
        admins.push_back(Address::generate(&env));

        let token_id = env.register_stellar_asset_contract_v2(admins.get(0).unwrap().clone());
        let contract_id = env.register_contract(None, QuorumCreditContract);

        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &1_000_000_000);

        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &1u32, &token_id.address());

        // Start at t=120 so all vouches pass MIN_VOUCH_AGE.
        env.ledger().with_mut(|l| l.timestamp = 120);

        Setup {
            env,
            client,
            token: token_id.address(),
        }
    }

    /// Issue #1470: matching reveal succeeds and settles the confidential vouch.
    #[test]
    fn test_reveal_vouch_commitment_matching_succeeds() {
        let s = setup();
        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        let stake_amount = 500_000i128;
        StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &stake_amount);

        let blinding = Bytes::from_array(&s.env, b"blind-a");
        let commitment = zk_snarks::commit_amount(&s.env, stake_amount, b"blind-a").unwrap();
        let proof = zk_snarks::create_vouch_proof(
            &s.env, &voucher, &borrower, &s.token, stake_amount, true, false,
        );

        s.client.vouch_confidential(
            &voucher,
            &borrower,
            &stake_amount,
            &commitment,
            &proof,
            &s.token,
            &None,
        );

        s.client
            .reveal_vouch_commitment(&voucher, &borrower, &stake_amount, &blinding);
    }

    /// Issue #1470: revealing with a value that doesn't hash to the stored commitment fails.
    #[test]
    fn test_reveal_vouch_commitment_mismatched_reveal_fails() {
        let s = setup();
        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        let stake_amount = 500_000i128;
        StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &stake_amount);

        let commitment = zk_snarks::commit_amount(&s.env, stake_amount, b"blind-a").unwrap();
        let proof = zk_snarks::create_vouch_proof(
            &s.env, &voucher, &borrower, &s.token, stake_amount, true, false,
        );

        s.client.vouch_confidential(
            &voucher,
            &borrower,
            &stake_amount,
            &commitment,
            &proof,
            &s.token,
            &None,
        );

        // Wrong amount.
        let wrong_amount = stake_amount + 1;
        let blinding = Bytes::from_array(&s.env, b"blind-a");
        let result = s
            .client
            .try_reveal_vouch_commitment(&voucher, &borrower, &wrong_amount, &blinding);
        assert_eq!(result, Err(Ok(ContractError::CommitmentMismatch)));

        // Wrong blinding factor.
        let wrong_blinding = Bytes::from_array(&s.env, b"blind-b");
        let result = s
            .client
            .try_reveal_vouch_commitment(&voucher, &borrower, &stake_amount, &wrong_blinding);
        assert_eq!(result, Err(Ok(ContractError::CommitmentMismatch)));
    }

    /// Issue #1470: an already-revealed commitment cannot be revealed (replayed) again.
    #[test]
    fn test_reveal_vouch_commitment_replay_is_rejected() {
        let s = setup();
        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        let stake_amount = 500_000i128;
        StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &stake_amount);

        let blinding = Bytes::from_array(&s.env, b"blind-a");
        let commitment = zk_snarks::commit_amount(&s.env, stake_amount, b"blind-a").unwrap();
        let proof = zk_snarks::create_vouch_proof(
            &s.env, &voucher, &borrower, &s.token, stake_amount, true, false,
        );

        s.client.vouch_confidential(
            &voucher,
            &borrower,
            &stake_amount,
            &commitment,
            &proof,
            &s.token,
            &None,
        );

        s.client
            .reveal_vouch_commitment(&voucher, &borrower, &stake_amount, &blinding);

        let result = s
            .client
            .try_reveal_vouch_commitment(&voucher, &borrower, &stake_amount, &blinding);
        assert_eq!(result, Err(Ok(ContractError::CommitmentAlreadyRevealed)));
    }

    /// Issue #1470: matching reveal succeeds for a confidential loan request, and a replay
    /// of an already-revealed loan commitment is rejected.
    #[test]
    fn test_reveal_loan_commitment_matching_then_replay_rejected() {
        let s = setup();
        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        let stake = 1_000_000i128;
        let loan_amount = 200_000i128;
        let threshold = 500_000i128;

        StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &stake);
        s.client.vouch(&voucher, &borrower, &stake, &s.token, &None);

        let blinding = Bytes::from_array(&s.env, b"blind-loan");
        let commitment = zk_snarks::commit_amount(&s.env, loan_amount, b"blind-loan").unwrap();
        let proof = zk_snarks::create_loan_proof(
            &s.env, &borrower, &s.token, loan_amount, threshold, true, true,
        );

        s.client.request_loan_confidential(
            &borrower,
            &loan_amount,
            &commitment,
            &proof,
            &threshold,
            &String::from_str(&s.env, "confidential loan"),
            &s.token,
        );

        s.client
            .reveal_loan_commitment(&borrower, &loan_amount, &blinding);

        let result = s
            .client
            .try_reveal_loan_commitment(&borrower, &loan_amount, &blinding);
        assert_eq!(result, Err(Ok(ContractError::CommitmentAlreadyRevealed)));
    }
}

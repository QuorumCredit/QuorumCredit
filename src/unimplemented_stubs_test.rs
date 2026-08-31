//! Explicit behavior tests for the unimplemented stub functions in
//! `loan.rs` (issue #1394). None of these are exposed as contract entry
//! points (see docs/unimplemented-stubs.md for the full audit), so they are
//! exercised here as plain Rust functions inside `env.as_contract`, the same
//! way other tests reach non-entry-point helpers directly.
#[cfg(test)]
mod unimplemented_stubs_tests {
    use crate::errors::ContractError;
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

    fn setup() -> (Env, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let admins = Vec::from_array(&env, [admin.clone()]);
        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        let contract_id = env.register_contract(None, QuorumCreditContract);

        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token_id.address());

        (env, contract_id, token_id.address())
    }

    #[test]
    fn deposit_collateral_always_errors() {
        let (env, contract_id, token) = setup();
        let borrower = Address::generate(&env);

        let result = env.as_contract(&contract_id, || {
            crate::loan::deposit_collateral(env.clone(), borrower.clone(), 1_000, token.clone())
        });

        assert_eq!(result, Err(ContractError::InvalidStateTransition));
    }

    #[test]
    fn get_borrower_collateral_always_returns_zero() {
        let (env, contract_id, _token) = setup();
        let borrower = Address::generate(&env);

        let result = env.as_contract(&contract_id, || {
            crate::loan::get_borrower_collateral(env.clone(), borrower.clone())
        });

        assert_eq!(result, 0);
    }

    #[test]
    fn emit_repayment_reminders_is_a_no_op() {
        let (env, contract_id, _token) = setup();

        // No assertion beyond "does not panic" — there is no observable
        // state to check, since the function does nothing.
        env.as_contract(&contract_id, || {
            crate::loan::emit_repayment_reminders(env.clone());
        });
    }

    #[test]
    fn mint_reputation_nft_reports_success_but_mints_nothing() {
        let (env, contract_id, _token) = setup();
        let borrower = Address::generate(&env);

        let result = env.as_contract(&contract_id, || {
            crate::loan::mint_reputation_nft(env.clone(), borrower.clone())
        });

        // Reports Ok(()) — this is the "silent no-op" shape flagged by #1394:
        // success is reported despite nothing actually being minted.
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn send_repayment_reminder_reports_success_but_sends_nothing() {
        let (env, contract_id, _token) = setup();

        let result =
            env.as_contract(&contract_id, || crate::loan::send_repayment_reminder(env.clone(), 1));

        assert_eq!(result, Ok(()));
    }

    #[test]
    fn defer_payment_always_errors_after_auth_and_thaw_checks() {
        let (env, contract_id, _token) = setup();
        let borrower = Address::generate(&env);

        let result = env.as_contract(&contract_id, || {
            crate::loan::defer_payment(env.clone(), borrower.clone())
        });

        assert_eq!(result, Err(ContractError::InvalidStateTransition));
    }

    #[test]
    fn check_acceleration_always_errors() {
        let (env, contract_id, _token) = setup();
        let borrower = Address::generate(&env);

        let result = env.as_contract(&contract_id, || {
            crate::loan::check_acceleration(env.clone(), borrower.clone())
        });

        assert_eq!(result, Err(ContractError::InvalidStateTransition));
    }
}

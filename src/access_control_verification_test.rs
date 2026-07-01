//! Issue #952 ([#106] Security Test Automation)
//!
//! `access_control_verification_test.rs` (added in #913) claimed to verify
//! access control across the contract's functions, but every test in it only
//! asserted trivial facts about the `Config` struct (e.g. `cfg.slash_bps > 0`)
//! without ever calling the functions named in their own doc comments, and
//! without ever exercising `require_auth()`, `require_admin_approval()`, or
//! `require_admin_permission()`. None of it would catch a real auth
//! regression. It also referenced `Config` fields (`min_stake`,
//! `protocol_fee_bps`, `max_allowed_tokens`, `min_vouchers`,
//! `vouch_cooldown_secs`, `thaw_duration_secs`) that don't exist anywhere
//! else in the crate.
//!
//! This file replaces it with tests that actually invoke the relevant
//! functions and assert on real success/failure outcomes:
//!   - calls below the admin multisig threshold panic
//!   - signers who aren't registered admins are rejected outright
//!   - self-removal-by-signer protection (Issue #372) actually holds
//!   - read-only functions remain callable without any auth

#[cfg(test)]
mod tests {
    use crate::admin;
    use crate::types::{Config, DataKey, RateLimitConfig};
    use soroban_sdk::{testutils::Address as _, vec, Address, Env};

    /// Builds a real, fully-populated Config matching `initialize()`'s literal
    /// in lib.rs, with two registered admins and threshold = 2.
    fn setup_env_with_admins() -> (Env, Address, Address, Address) {
        let env = Env::default();

        let admin1 = Address::generate(&env);
        let admin2 = Address::generate(&env);
        let token = Address::generate(&env);
        let contract_id = env.register_contract(None, crate::QuorumCreditContract);

        let config = Config {
            admins: vec![&env, admin1.clone(), admin2.clone()],
            admin_threshold: 2,
            admin_whitelist: vec![&env],
            admin_blacklist: vec![&env],
            token: token.clone(),
            allowed_tokens: vec![&env],
            yield_bps: 200,
            slash_bps: 5000,
            max_vouchers: 100,
            min_loan_amount: 100_000,
            loan_duration: 30 * 24 * 60 * 60,
            max_loan_to_stake_ratio: 150,
            grace_period: 0,
            min_vouch_age_secs: 24 * 60 * 60,
            prepayment_penalty_bps: 0,
            liquidity_mining_rate_bps: 0,
            voting_period_seconds: 14 * 24 * 60 * 60,
            slash_cooldown_seconds: 0,
            emergency_pause_enabled: false,
            early_repayment_discount_bps: 0,
            oracle_address: None,
            slash_delay_seconds: 0,
            successor_admin: None,
            rate_limit_config: RateLimitConfig {
                window_secs: 3600,
                max_calls: 1000,
                enabled: false,
            },
            multi_tier_thresholds: None,
        };

        env.as_contract(&contract_id, || {
            env.storage().instance().set(&DataKey::Config, &config);
        });

        (env, admin1, admin2, token)
    }

    // ── Multisig threshold is genuinely enforced ────────────────────────────────

    /// A single admin signer cannot pass `require_admin_approval` when the
    /// threshold is 2 — this should panic on the "insufficient admin
    /// approvals" assertion, proving the threshold check is live, not
    /// decorative.
    #[test]
    #[should_panic(expected = "insufficient admin approvals")]
    fn set_protocol_fee_rejects_below_threshold_signers() {
        let (env, admin1, _admin2, _token) = setup_env_with_admins();
        let contract_id = env.register_contract(None, crate::QuorumCreditContract);

        env.as_contract(&contract_id, || {
            admin::set_protocol_fee(env.clone(), vec![&env, admin1], 250);
        });
    }

    /// Meeting the threshold with two genuine registered admins succeeds.
    /// Mirrors the previous test but proves the positive path also works,
    /// not just that everything panics.
    #[test]
    fn set_protocol_fee_accepts_threshold_met_by_registered_admins() {
        let (env, admin1, admin2, _token) = setup_env_with_admins();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, crate::QuorumCreditContract);

        env.as_contract(&contract_id, || {
            admin::set_protocol_fee(env.clone(), vec![&env, admin1, admin2], 250);
        });
    }

    /// An address that is NOT in `cfg.admins` cannot be used to satisfy the
    /// multisig requirement, even if enough total signers are supplied to
    /// meet the numeric threshold.
    #[test]
    #[should_panic(expected = "signer is not a registered admin")]
    fn set_protocol_fee_rejects_non_admin_signer() {
        let (env, admin1, _admin2, _token) = setup_env_with_admins();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, crate::QuorumCreditContract);
        let outsider = Address::generate(&env);

        env.as_contract(&contract_id, || {
            admin::set_protocol_fee(env.clone(), vec![&env, admin1, outsider], 250);
        });
    }

    // ── add_admin / remove_admin enforce the same multisig gate ─────────────────

    #[test]
    #[should_panic(expected = "insufficient admin approvals")]
    fn add_admin_rejects_below_threshold_signers() {
        let (env, admin1, _admin2, _token) = setup_env_with_admins();
        let contract_id = env.register_contract(None, crate::QuorumCreditContract);
        let new_admin = Address::generate(&env);

        env.as_contract(&contract_id, || {
            admin::add_admin(env.clone(), vec![&env, admin1], new_admin);
        });
    }

    /// An admin cannot be used as a signer to remove themselves — this is
    /// the Issue #372 protection already noted in admin.rs; we assert it
    /// still holds rather than assuming the comment is accurate.
    #[test]
    #[should_panic]
    fn remove_admin_rejects_self_removal_by_signer() {
        let (env, admin1, admin2, _token) = setup_env_with_admins();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, crate::QuorumCreditContract);

        env.as_contract(&contract_id, || {
            // admin1 is both a signer and the target of removal.
            admin::remove_admin(env.clone(), vec![&env, admin1.clone(), admin2], admin1);
        });
    }

    // ── Read functions remain unauthenticated (sanity, not a vacuous check) ─────

    /// get_governance_proposal is a read-only query and must not require any
    /// signature. We call it directly (no mock_all_auths, no signers) and
    /// confirm it simply returns None rather than panicking on auth.
    #[test]
    fn get_governance_proposal_requires_no_auth() {
        let (env, _admin1, _admin2, _token) = setup_env_with_admins();
        let contract_id = env.register_contract(None, crate::QuorumCreditContract);

        let result = env.as_contract(&contract_id, || {
            admin::get_governance_proposal(env.clone(), 999)
        });

        assert!(result.is_none());
    }
}

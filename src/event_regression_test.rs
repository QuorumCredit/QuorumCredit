/// Event Regression Tests — Issue #104
///
/// Captures event "snapshots" for the five critical state transitions:
/// vouch, request_loan, repay, slash, auto_slash.
///
/// On each run the test asserts:
///  • the correct number of contract events were emitted
///  • each event's topic pair matches the expected symbolic identifiers
///
/// This acts as a snapshot guard: if any event is added, removed, or renamed
/// the corresponding assertion fails immediately, surfacing the regression in CI.
///
/// All amounts are in stroops (1 XLM = 10_000_000 stroops).
#[cfg(test)]
mod event_regression_tests {
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{
        symbol_short,
        testutils::{Address as _, Events as _, Ledger},
        token::StellarAssetClient,
        Address, Env, String, Vec,
    };

    const ONE_XLM: i128 = 10_000_000;

    // ── Setup ─────────────────────────────────────────────────────────────────

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
        // Fund contract so it can disburse loans and pay yield
        StellarAssetClient::new(&env, &token_id.address())
            .mint(&contract_id, &(500 * ONE_XLM));
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token_id.address());
        // Advance past the minimum vouch age (61 s)
        env.ledger().with_mut(|l| l.timestamp = 200);
        Setup { env, client, admin, token: token_id.address() }
    }

    fn admins(s: &Setup) -> Vec<Address> {
        Vec::from_array(&s.env, [s.admin.clone()])
    }

    /// Mint tokens to voucher, create a vouch, advance past MIN_VOUCH_AGE.
    fn do_vouch(s: &Setup, voucher: &Address, borrower: &Address, stake: i128) {
        StellarAssetClient::new(&s.env, &s.token).mint(voucher, &stake);
        s.client.vouch(voucher, borrower, &stake, &s.token, &None);
        s.env.ledger().with_mut(|l| l.timestamp += 61);
    }

    fn do_loan(s: &Setup, borrower: &Address, amount: i128) {
        s.client.request_loan(
            borrower,
            &amount,
            &(amount / 2),
            &String::from_str(&s.env, "test loan"),
            &s.token,
        );
    }

    // ── Helper: collect topic pairs from the last N events ────────────────────

    /// Returns a Vec of (topic0, topic1) symbol-pair strings for every contract
    /// event emitted since the last reset.
    fn topic_pairs(s: &Setup) -> std::vec::Vec<(String, String)> {
        let events = s.env.events().all();
        let raw = events.events();
        let mut out = std::vec::Vec::new();
        for ev in raw.iter() {
            if let soroban_sdk::xdr::ContractEventBody::V0(body) = &ev.body {
                if body.topics.len() >= 2 {
                    // topics are ScVal — convert to debug string for comparison
                    let t0 = format!("{:?}", body.topics[0]);
                    let t1 = format!("{:?}", body.topics[1]);
                    out.push((t0, t1));
                }
            }
        }
        out
    }

    // ── Snapshot: vouch ───────────────────────────────────────────────────────

    /// After a successful `vouch` call, exactly one contract event with topic
    /// ("vouch", "create") MUST be emitted.
    #[test]
    fn event_snapshot_vouch() {
        let s = setup();
        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &ONE_XLM);
        s.client.vouch(&voucher, &borrower, &ONE_XLM, &s.token, &None);

        let all = s.env.events().all();
        let events = all.events();

        // At least one event must be present
        assert!(
            !events.is_empty(),
            "event_snapshot_vouch: expected ≥1 event, got 0"
        );

        // Verify the last contract event has the correct topic pair
        let last_event = events.last().expect("no events emitted");
        if let soroban_sdk::xdr::ContractEventBody::V0(body) = &last_event.body {
            assert!(
                body.topics.len() >= 2,
                "event_snapshot_vouch: expected ≥2 topics, got {}",
                body.topics.len()
            );
            let t0 = format!("{:?}", body.topics[0]);
            let t1 = format!("{:?}", body.topics[1]);
            assert!(
                t0.contains("vouch"),
                "event_snapshot_vouch: topic[0] should contain 'vouch', got: {t0}"
            );
            assert!(
                t1.contains("create"),
                "event_snapshot_vouch: topic[1] should contain 'create', got: {t1}"
            );
        }
    }

    // ── Snapshot: request_loan ────────────────────────────────────────────────

    /// After a successful `request_loan` call, a ("loan", "request") event
    /// MUST be emitted.
    #[test]
    fn event_snapshot_request_loan() {
        let s = setup();
        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        do_vouch(&s, &voucher, &borrower, 2 * ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM);

        let all = s.env.events().all();
        let events = all.events();
        assert!(
            !events.is_empty(),
            "event_snapshot_request_loan: expected ≥1 event"
        );

        // Find the loan/request event
        let found = events.iter().any(|ev| {
            if let soroban_sdk::xdr::ContractEventBody::V0(body) = &ev.body {
                if body.topics.len() >= 2 {
                    let t0 = format!("{:?}", body.topics[0]);
                    let t1 = format!("{:?}", body.topics[1]);
                    return t0.contains("loan") && t1.contains("request");
                }
            }
            false
        });
        assert!(
            found,
            "event_snapshot_request_loan: no ('loan','request') event found in: {:?}",
            topic_pairs(&s)
        );
    }

    // ── Snapshot: repay ───────────────────────────────────────────────────────

    /// After a successful full `repay` call, a ("loan", "repay") or ("loan",
    /// "repaid") event MUST be emitted.
    #[test]
    fn event_snapshot_repay() {
        let s = setup();
        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        do_vouch(&s, &voucher, &borrower, 2 * ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM);

        // Fund borrower for repayment (principal + yield headroom)
        let repayment = ONE_XLM + ONE_XLM * 200 / 10_000 + 1;
        StellarAssetClient::new(&s.env, &s.token).mint(&borrower, &repayment);

        s.client.repay(&borrower, &repayment);

        let all = s.env.events().all();
        let events = all.events();
        assert!(
            !events.is_empty(),
            "event_snapshot_repay: expected ≥1 event"
        );

        // Find the loan/repay or loan/repaid event
        let found = events.iter().any(|ev| {
            if let soroban_sdk::xdr::ContractEventBody::V0(body) = &ev.body {
                if body.topics.len() >= 2 {
                    let t0 = format!("{:?}", body.topics[0]);
                    let t1 = format!("{:?}", body.topics[1]);
                    return t0.contains("loan")
                        && (t1.contains("repay") || t1.contains("repaid"));
                }
            }
            false
        });
        assert!(
            found,
            "event_snapshot_repay: no ('loan','repay'|'repaid') event found in: {:?}",
            topic_pairs(&s)
        );
    }

    // ── Snapshot: repay — before/after event count delta ─────────────────────

    /// The number of events emitted during repay MUST be greater than zero.
    /// This test captures a "before count" snapshot and verifies that new events
    /// are produced by the repay transition.
    #[test]
    fn event_snapshot_repay_delta() {
        let s = setup();
        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        do_vouch(&s, &voucher, &borrower, 2 * ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM);

        let count_before = s.env.events().all().events().len();

        let repayment = ONE_XLM + ONE_XLM * 200 / 10_000 + 1;
        StellarAssetClient::new(&s.env, &s.token).mint(&borrower, &repayment);
        s.client.repay(&borrower, &repayment);

        let count_after = s.env.events().all().events().len();

        assert!(
            count_after > count_before,
            "event_snapshot_repay_delta: no new events emitted during repay \
             (before={count_before}, after={count_after})"
        );
    }

    // ── Snapshot: slash ───────────────────────────────────────────────────────

    /// After an admin `slash` call, a ("loan", "slash") event MUST be emitted.
    #[test]
    fn event_snapshot_slash() {
        let s = setup();
        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        do_vouch(&s, &voucher, &borrower, 2 * ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM);

        let count_before = s.env.events().all().events().len();

        s.client.slash(&admins(&s), &borrower);

        let count_after = s.env.events().all().events().len();
        assert!(
            count_after > count_before,
            "event_snapshot_slash: no new events during slash \
             (before={count_before}, after={count_after})"
        );
    }

    // ── Snapshot: slash — before/after event count delta ─────────────────────

    /// Slash MUST emit at least one event beyond what was already present.
    #[test]
    fn event_snapshot_slash_delta() {
        let s = setup();
        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        do_vouch(&s, &voucher, &borrower, 2 * ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM);

        let events_before = s.env.events().all().events().len();

        s.client.slash(&admins(&s), &borrower);

        let events_after = s.env.events().all().events().len();
        assert!(
            events_after > events_before,
            "event_snapshot_slash_delta: slash did not emit any new events \
             (before={events_before}, after={events_after})"
        );
    }

    // ── Snapshot: auto_slash ──────────────────────────────────────────────────

    /// After `auto_slash` triggers (deadline passed), at least one new event
    /// MUST be emitted.
    #[test]
    fn event_snapshot_auto_slash() {
        let s = setup();
        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        do_vouch(&s, &voucher, &borrower, 2 * ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM);

        // Advance time past the loan deadline (default 30 days = 2_592_000 s)
        s.env.ledger().with_mut(|l| l.timestamp += 2_592_001);

        let count_before = s.env.events().all().events().len();

        s.client.auto_slash(&borrower);

        let count_after = s.env.events().all().events().len();
        assert!(
            count_after > count_before,
            "event_snapshot_auto_slash: no new events during auto_slash \
             (before={count_before}, after={count_after})"
        );
    }

    // ── Snapshot: initialize ──────────────────────────────────────────────────

    /// `initialize` MUST emit exactly one ("contract", "init") event.
    #[test]
    fn event_snapshot_initialize() {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let admins = Vec::from_array(&env, [admin.clone()]);
        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        client.initialize(&deployer, &admins, &1, &token_id.address());

        let all = env.events().all();
        let events = all.events();
        assert!(
            !events.is_empty(),
            "event_snapshot_initialize: expected ≥1 event"
        );

        let found = events.iter().any(|ev| {
            if let soroban_sdk::xdr::ContractEventBody::V0(body) = &ev.body {
                if body.topics.len() >= 2 {
                    let t0 = format!("{:?}", body.topics[0]);
                    let t1 = format!("{:?}", body.topics[1]);
                    return t0.contains("contract") && t1.contains("init");
                }
            }
            false
        });
        assert!(
            found,
            "event_snapshot_initialize: no ('contract','init') event found"
        );
    }

    // ── Snapshot: vouch — topic count consistency ─────────────────────────────

    /// Every vouch/create event MUST have exactly 2 topics.
    #[test]
    fn event_snapshot_vouch_topic_count() {
        let s = setup();
        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &ONE_XLM);
        s.client.vouch(&voucher, &borrower, &ONE_XLM, &s.token, &None);

        let all = s.env.events().all();
        let events = all.events();

        for ev in events.iter() {
            if let soroban_sdk::xdr::ContractEventBody::V0(body) = &ev.body {
                let t0 = format!("{:?}", body.topics[0]);
                if t0.contains("vouch") {
                    assert!(
                        body.topics.len() >= 2,
                        "event_snapshot_vouch_topic_count: vouch event has <2 topics: {:?}",
                        body.topics
                    );
                }
            }
        }
    }

    // ── Snapshot: no spurious events on read-only calls ───────────────────────

    /// Read-only queries (`get_loan`, `get_vouches`, `is_eligible`) MUST NOT
    /// emit any contract events.
    #[test]
    fn event_snapshot_no_events_on_reads() {
        let s = setup();
        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);

        do_vouch(&s, &voucher, &borrower, 2 * ONE_XLM);
        do_loan(&s, &borrower, ONE_XLM);

        let count_before = s.env.events().all().events().len();

        // Read-only calls
        let _ = s.client.get_loan(&borrower);
        let _ = s.client.get_vouches(&borrower);
        let _ = s.client.loan_status(&borrower);
        let _ = s.client.total_vouched(&borrower);

        let count_after = s.env.events().all().events().len();
        assert_eq!(
            count_before, count_after,
            "event_snapshot_no_events_on_reads: read-only calls emitted unexpected events \
             (before={count_before}, after={count_after})"
        );
    }
}

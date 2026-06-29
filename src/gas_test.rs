/// Gas / Performance Regression Tests — Issue #105
///
/// Measures CPU instruction count and memory bytes consumed by each core
/// contract function using the Soroban SDK's `env.cost_estimate().budget()` API.
///
/// # How to run measurement tests and update budgets
///
/// 1. Run measurements (prints CPU + memory to stdout):
///    ```
///    cargo test --lib gas -- --nocapture
///    ```
/// 2. Observe the printed baselines.
/// 3. Update the `CPU_BUDGET_*` / `MEM_BUDGET_*` constants below to
///    `measured_value * 1.5`, rounded up to the nearest 1_000.
/// 4. Re-run `cargo test --lib gas` to confirm all regression tests pass.
///
/// Scenarios
/// ---------
///  * "typical"  — 1 voucher backing the borrower
///  * "worst"    — DEFAULT_MAX_VOUCHERS_PER_BORROWER (50) vouchers
///
/// All amounts are in stroops (1 XLM = 10_000_000 stroops).
#[cfg(test)]
mod gas_tests {
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, Env, String, Vec,
    };

    const ONE_XLM: i128 = 10_000_000;
    /// Maximum vouchers per borrower (mirrors DEFAULT_MAX_VOUCHERS in types.rs).
    const MAX_VOUCHERS: u32 = 50;

    // ── Budget constants ──────────────────────────────────────────────────────
    // Set conservatively at measured_baseline × 1.5, rounded up to 1_000.
    // The Soroban test runtime underestimates vs. WASM; these values are
    // intentionally generous to avoid spurious CI failures.

    const CPU_BUDGET_VOUCH_TYPICAL: u64      = 3_000_000;
    const MEM_BUDGET_VOUCH_TYPICAL: u64      = 3_000_000;
    const CPU_BUDGET_VOUCH_WORST: u64        = 5_000_000;
    const MEM_BUDGET_VOUCH_WORST: u64        = 5_000_000;

    const CPU_BUDGET_REQUEST_LOAN_TYPICAL: u64 = 4_000_000;
    const MEM_BUDGET_REQUEST_LOAN_TYPICAL: u64 = 4_000_000;
    const CPU_BUDGET_REQUEST_LOAN_WORST: u64   = 7_000_000;
    const MEM_BUDGET_REQUEST_LOAN_WORST: u64   = 7_000_000;

    const CPU_BUDGET_REPAY_TYPICAL: u64      = 5_000_000;
    const MEM_BUDGET_REPAY_TYPICAL: u64      = 5_000_000;
    const CPU_BUDGET_REPAY_WORST: u64        = 15_000_000;
    const MEM_BUDGET_REPAY_WORST: u64        = 15_000_000;

    const CPU_BUDGET_SLASH_TYPICAL: u64      = 5_000_000;
    const MEM_BUDGET_SLASH_TYPICAL: u64      = 5_000_000;
    const CPU_BUDGET_SLASH_WORST: u64        = 15_000_000;
    const MEM_BUDGET_SLASH_WORST: u64        = 15_000_000;

    const CPU_BUDGET_AUTO_SLASH_TYPICAL: u64 = 5_000_000;
    const MEM_BUDGET_AUTO_SLASH_TYPICAL: u64 = 5_000_000;
    const CPU_BUDGET_AUTO_SLASH_WORST: u64   = 15_000_000;
    const MEM_BUDGET_AUTO_SLASH_WORST: u64   = 15_000_000;

    const CPU_BUDGET_WITHDRAW_VOUCH: u64     = 4_000_000;
    const MEM_BUDGET_WITHDRAW_VOUCH: u64     = 4_000_000;

    const CPU_BUDGET_BATCH_VOUCH_WORST: u64  = 60_000_000;
    const MEM_BUDGET_BATCH_VOUCH_WORST: u64  = 60_000_000;

    // ── Shared fixture ────────────────────────────────────────────────────────

    struct Fixture {
        env: Env,
        client: QuorumCreditContractClient<'static>,
        admin: Address,
        token: Address,
        contract_id: Address,
    }

    fn setup() -> Fixture {
        let env = Env::default();
        env.mock_all_auths();
        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let admins = Vec::from_array(&env, [admin.clone()]);
        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        let contract_id = env.register_contract(None, QuorumCreditContract);
        // Pre-fund the contract for loans and yield
        StellarAssetClient::new(&env, &token_id.address())
            .mint(&contract_id, &(10_000 * ONE_XLM));
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token_id.address());
        env.ledger().with_mut(|l| l.timestamp = 1_000);
        Fixture { env, client, admin, token: token_id.address(), contract_id }
    }

    fn admins(f: &Fixture) -> Vec<Address> {
        Vec::from_array(&f.env, [f.admin.clone()])
    }

    /// Mint tokens to voucher, vouch, and advance past MIN_VOUCH_AGE (61 s).
    fn add_vouch(f: &Fixture, voucher: &Address, borrower: &Address, stake: i128) {
        StellarAssetClient::new(&f.env, &f.token).mint(voucher, &stake);
        f.client.vouch(voucher, borrower, &stake, &f.token, &None);
        f.env.ledger().with_mut(|l| l.timestamp += 62);
    }

    fn do_loan(f: &Fixture, borrower: &Address, amount: i128) {
        f.client.request_loan(
            borrower,
            &amount,
            &(amount / 2),
            &String::from_str(&f.env, "gas test"),
            &f.token,
        );
    }

    // ── Measurement helpers ───────────────────────────────────────────────────

    fn read_budget(f: &Fixture) -> (u64, u64) {
        let budget = f.env.cost_estimate().budget();
        (budget.cpu_instruction_cost(), budget.memory_bytes_cost())
    }

    fn reset_budget(f: &Fixture) {
        f.env.cost_estimate().budget().reset_default();
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // MEASUREMENT TESTS — run with `cargo test gas -- --nocapture` to view values
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn measure_vouch_typical() {
        let f = setup();
        let voucher = Address::generate(&f.env);
        let borrower = Address::generate(&f.env);
        StellarAssetClient::new(&f.env, &f.token).mint(&voucher, &ONE_XLM);

        reset_budget(&f);
        f.client.vouch(&voucher, &borrower, &ONE_XLM, &f.token, &None);
        let (cpu, mem) = read_budget(&f);
        println!("vouch [typical 1 voucher] cpu={cpu} mem={mem}");
    }

    #[test]
    fn measure_vouch_worst() {
        let f = setup();
        let borrower = Address::generate(&f.env);
        // Add MAX_VOUCHERS - 1 prior vouches so the next vouch is the worst case
        for _ in 0..MAX_VOUCHERS - 1 {
            let v = Address::generate(&f.env);
            add_vouch(&f, &v, &borrower, ONE_XLM);
        }
        let final_voucher = Address::generate(&f.env);
        StellarAssetClient::new(&f.env, &f.token).mint(&final_voucher, &ONE_XLM);

        reset_budget(&f);
        f.client.vouch(&final_voucher, &borrower, &ONE_XLM, &f.token, &None);
        let (cpu, mem) = read_budget(&f);
        println!("vouch [worst {} vouchers] cpu={cpu} mem={mem}", MAX_VOUCHERS);
    }

    #[test]
    fn measure_request_loan_typical() {
        let f = setup();
        let voucher = Address::generate(&f.env);
        let borrower = Address::generate(&f.env);
        add_vouch(&f, &voucher, &borrower, 2 * ONE_XLM);

        reset_budget(&f);
        f.client.request_loan(
            &borrower,
            &ONE_XLM,
            &ONE_XLM,
            &String::from_str(&f.env, "measure"),
            &f.token,
        );
        let (cpu, mem) = read_budget(&f);
        println!("request_loan [typical 1 voucher] cpu={cpu} mem={mem}");
    }

    #[test]
    fn measure_request_loan_worst() {
        let f = setup();
        let borrower = Address::generate(&f.env);
        for _ in 0..MAX_VOUCHERS {
            let v = Address::generate(&f.env);
            add_vouch(&f, &v, &borrower, ONE_XLM);
        }

        reset_budget(&f);
        f.client.request_loan(
            &borrower,
            &ONE_XLM,
            &ONE_XLM,
            &String::from_str(&f.env, "measure worst"),
            &f.token,
        );
        let (cpu, mem) = read_budget(&f);
        println!("request_loan [worst {} vouchers] cpu={cpu} mem={mem}", MAX_VOUCHERS);
    }

    #[test]
    fn measure_repay_typical() {
        let f = setup();
        let voucher = Address::generate(&f.env);
        let borrower = Address::generate(&f.env);
        add_vouch(&f, &voucher, &borrower, 2 * ONE_XLM);
        do_loan(&f, &borrower, ONE_XLM);
        let repayment = ONE_XLM + ONE_XLM * 200 / 10_000 + 1;
        StellarAssetClient::new(&f.env, &f.token).mint(&borrower, &repayment);

        reset_budget(&f);
        f.client.repay(&borrower, &repayment);
        let (cpu, mem) = read_budget(&f);
        println!("repay [typical 1 voucher] cpu={cpu} mem={mem}");
    }

    #[test]
    fn measure_repay_worst() {
        let f = setup();
        let borrower = Address::generate(&f.env);
        for _ in 0..MAX_VOUCHERS {
            let v = Address::generate(&f.env);
            add_vouch(&f, &v, &borrower, ONE_XLM);
        }
        do_loan(&f, &borrower, ONE_XLM);
        let repayment = ONE_XLM + ONE_XLM * 200 / 10_000 + 1;
        StellarAssetClient::new(&f.env, &f.token).mint(&borrower, &repayment);

        reset_budget(&f);
        f.client.repay(&borrower, &repayment);
        let (cpu, mem) = read_budget(&f);
        println!("repay [worst {} vouchers] cpu={cpu} mem={mem}", MAX_VOUCHERS);
    }

    #[test]
    fn measure_slash_typical() {
        let f = setup();
        let voucher = Address::generate(&f.env);
        let borrower = Address::generate(&f.env);
        add_vouch(&f, &voucher, &borrower, 2 * ONE_XLM);
        do_loan(&f, &borrower, ONE_XLM);

        reset_budget(&f);
        f.client.slash(&admins(&f), &borrower);
        let (cpu, mem) = read_budget(&f);
        println!("slash [typical 1 voucher] cpu={cpu} mem={mem}");
    }

    #[test]
    fn measure_slash_worst() {
        let f = setup();
        let borrower = Address::generate(&f.env);
        for _ in 0..MAX_VOUCHERS {
            let v = Address::generate(&f.env);
            add_vouch(&f, &v, &borrower, ONE_XLM);
        }
        do_loan(&f, &borrower, ONE_XLM);

        reset_budget(&f);
        f.client.slash(&admins(&f), &borrower);
        let (cpu, mem) = read_budget(&f);
        println!("slash [worst {} vouchers] cpu={cpu} mem={mem}", MAX_VOUCHERS);
    }

    #[test]
    fn measure_auto_slash_typical() {
        let f = setup();
        let voucher = Address::generate(&f.env);
        let borrower = Address::generate(&f.env);
        add_vouch(&f, &voucher, &borrower, 2 * ONE_XLM);
        do_loan(&f, &borrower, ONE_XLM);
        f.env.ledger().with_mut(|l| l.timestamp += 2_592_001);

        reset_budget(&f);
        f.client.auto_slash(&borrower);
        let (cpu, mem) = read_budget(&f);
        println!("auto_slash [typical 1 voucher] cpu={cpu} mem={mem}");
    }

    #[test]
    fn measure_auto_slash_worst() {
        let f = setup();
        let borrower = Address::generate(&f.env);
        for _ in 0..MAX_VOUCHERS {
            let v = Address::generate(&f.env);
            add_vouch(&f, &v, &borrower, ONE_XLM);
        }
        do_loan(&f, &borrower, ONE_XLM);
        f.env.ledger().with_mut(|l| l.timestamp += 2_592_001);

        reset_budget(&f);
        f.client.auto_slash(&borrower);
        let (cpu, mem) = read_budget(&f);
        println!("auto_slash [worst {} vouchers] cpu={cpu} mem={mem}", MAX_VOUCHERS);
    }

    #[test]
    fn measure_withdraw_vouch() {
        let f = setup();
        let voucher = Address::generate(&f.env);
        let borrower = Address::generate(&f.env);
        add_vouch(&f, &voucher, &borrower, ONE_XLM);
        // No active loan — withdrawal executes immediately

        reset_budget(&f);
        f.client.withdraw_vouch(&voucher, &borrower);
        let (cpu, mem) = read_budget(&f);
        println!("withdraw_vouch [typical] cpu={cpu} mem={mem}");
    }

    #[test]
    fn measure_batch_vouch_worst() {
        let f = setup();
        let voucher = Address::generate(&f.env);
        StellarAssetClient::new(&f.env, &f.token)
            .mint(&voucher, &(MAX_VOUCHERS as i128 * ONE_XLM));

        let mut borrowers = Vec::new(&f.env);
        let mut stakes = Vec::new(&f.env);
        for _ in 0..MAX_VOUCHERS {
            borrowers.push_back(Address::generate(&f.env));
            stakes.push_back(ONE_XLM);
        }

        reset_budget(&f);
        f.client.batch_vouch(&voucher, &borrowers, &stakes, &f.token, &None);
        let (cpu, mem) = read_budget(&f);
        println!("batch_vouch [worst {} borrowers] cpu={cpu} mem={mem}", MAX_VOUCHERS);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // REGRESSION TESTS — assert CPU ≤ budget AND memory ≤ budget
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn regression_vouch_typical() {
        let f = setup();
        let voucher = Address::generate(&f.env);
        let borrower = Address::generate(&f.env);
        StellarAssetClient::new(&f.env, &f.token).mint(&voucher, &ONE_XLM);

        reset_budget(&f);
        f.client.vouch(&voucher, &borrower, &ONE_XLM, &f.token, &None);
        let (cpu, mem) = read_budget(&f);

        assert!(
            cpu <= CPU_BUDGET_VOUCH_TYPICAL,
            "regression_vouch_typical: CPU regression {cpu} > budget {CPU_BUDGET_VOUCH_TYPICAL}"
        );
        assert!(
            mem <= MEM_BUDGET_VOUCH_TYPICAL,
            "regression_vouch_typical: MEM regression {mem} > budget {MEM_BUDGET_VOUCH_TYPICAL}"
        );
    }

    #[test]
    fn regression_vouch_worst() {
        let f = setup();
        let borrower = Address::generate(&f.env);
        for _ in 0..MAX_VOUCHERS - 1 {
            let v = Address::generate(&f.env);
            add_vouch(&f, &v, &borrower, ONE_XLM);
        }
        let final_voucher = Address::generate(&f.env);
        StellarAssetClient::new(&f.env, &f.token).mint(&final_voucher, &ONE_XLM);

        reset_budget(&f);
        f.client.vouch(&final_voucher, &borrower, &ONE_XLM, &f.token, &None);
        let (cpu, mem) = read_budget(&f);

        assert!(
            cpu <= CPU_BUDGET_VOUCH_WORST,
            "regression_vouch_worst: CPU regression {cpu} > budget {CPU_BUDGET_VOUCH_WORST}"
        );
        assert!(
            mem <= MEM_BUDGET_VOUCH_WORST,
            "regression_vouch_worst: MEM regression {mem} > budget {MEM_BUDGET_VOUCH_WORST}"
        );
    }

    #[test]
    fn regression_request_loan_typical() {
        let f = setup();
        let voucher = Address::generate(&f.env);
        let borrower = Address::generate(&f.env);
        add_vouch(&f, &voucher, &borrower, 2 * ONE_XLM);

        reset_budget(&f);
        f.client.request_loan(
            &borrower,
            &ONE_XLM,
            &ONE_XLM,
            &String::from_str(&f.env, "regression"),
            &f.token,
        );
        let (cpu, mem) = read_budget(&f);

        assert!(
            cpu <= CPU_BUDGET_REQUEST_LOAN_TYPICAL,
            "regression_request_loan_typical: CPU {cpu} > {CPU_BUDGET_REQUEST_LOAN_TYPICAL}"
        );
        assert!(
            mem <= MEM_BUDGET_REQUEST_LOAN_TYPICAL,
            "regression_request_loan_typical: MEM {mem} > {MEM_BUDGET_REQUEST_LOAN_TYPICAL}"
        );
    }

    #[test]
    fn regression_request_loan_worst() {
        let f = setup();
        let borrower = Address::generate(&f.env);
        for _ in 0..MAX_VOUCHERS {
            let v = Address::generate(&f.env);
            add_vouch(&f, &v, &borrower, ONE_XLM);
        }

        reset_budget(&f);
        f.client.request_loan(
            &borrower,
            &ONE_XLM,
            &ONE_XLM,
            &String::from_str(&f.env, "regression worst"),
            &f.token,
        );
        let (cpu, mem) = read_budget(&f);

        assert!(
            cpu <= CPU_BUDGET_REQUEST_LOAN_WORST,
            "regression_request_loan_worst: CPU {cpu} > {CPU_BUDGET_REQUEST_LOAN_WORST}"
        );
        assert!(
            mem <= MEM_BUDGET_REQUEST_LOAN_WORST,
            "regression_request_loan_worst: MEM {mem} > {MEM_BUDGET_REQUEST_LOAN_WORST}"
        );
    }

    #[test]
    fn regression_repay_typical() {
        let f = setup();
        let voucher = Address::generate(&f.env);
        let borrower = Address::generate(&f.env);
        add_vouch(&f, &voucher, &borrower, 2 * ONE_XLM);
        do_loan(&f, &borrower, ONE_XLM);
        let repayment = ONE_XLM + ONE_XLM * 200 / 10_000 + 1;
        StellarAssetClient::new(&f.env, &f.token).mint(&borrower, &repayment);

        reset_budget(&f);
        f.client.repay(&borrower, &repayment);
        let (cpu, mem) = read_budget(&f);

        assert!(
            cpu <= CPU_BUDGET_REPAY_TYPICAL,
            "regression_repay_typical: CPU {cpu} > {CPU_BUDGET_REPAY_TYPICAL}"
        );
        assert!(
            mem <= MEM_BUDGET_REPAY_TYPICAL,
            "regression_repay_typical: MEM {mem} > {MEM_BUDGET_REPAY_TYPICAL}"
        );
    }

    #[test]
    fn regression_repay_worst() {
        let f = setup();
        let borrower = Address::generate(&f.env);
        for _ in 0..MAX_VOUCHERS {
            let v = Address::generate(&f.env);
            add_vouch(&f, &v, &borrower, ONE_XLM);
        }
        do_loan(&f, &borrower, ONE_XLM);
        let repayment = ONE_XLM + ONE_XLM * 200 / 10_000 + 1;
        StellarAssetClient::new(&f.env, &f.token).mint(&borrower, &repayment);

        reset_budget(&f);
        f.client.repay(&borrower, &repayment);
        let (cpu, mem) = read_budget(&f);

        assert!(
            cpu <= CPU_BUDGET_REPAY_WORST,
            "regression_repay_worst: CPU {cpu} > {CPU_BUDGET_REPAY_WORST}"
        );
        assert!(
            mem <= MEM_BUDGET_REPAY_WORST,
            "regression_repay_worst: MEM {mem} > {MEM_BUDGET_REPAY_WORST}"
        );
    }

    #[test]
    fn regression_slash_typical() {
        let f = setup();
        let voucher = Address::generate(&f.env);
        let borrower = Address::generate(&f.env);
        add_vouch(&f, &voucher, &borrower, 2 * ONE_XLM);
        do_loan(&f, &borrower, ONE_XLM);

        reset_budget(&f);
        f.client.slash(&admins(&f), &borrower);
        let (cpu, mem) = read_budget(&f);

        assert!(
            cpu <= CPU_BUDGET_SLASH_TYPICAL,
            "regression_slash_typical: CPU {cpu} > {CPU_BUDGET_SLASH_TYPICAL}"
        );
        assert!(
            mem <= MEM_BUDGET_SLASH_TYPICAL,
            "regression_slash_typical: MEM {mem} > {MEM_BUDGET_SLASH_TYPICAL}"
        );
    }

    #[test]
    fn regression_slash_worst() {
        let f = setup();
        let borrower = Address::generate(&f.env);
        for _ in 0..MAX_VOUCHERS {
            let v = Address::generate(&f.env);
            add_vouch(&f, &v, &borrower, ONE_XLM);
        }
        do_loan(&f, &borrower, ONE_XLM);

        reset_budget(&f);
        f.client.slash(&admins(&f), &borrower);
        let (cpu, mem) = read_budget(&f);

        assert!(
            cpu <= CPU_BUDGET_SLASH_WORST,
            "regression_slash_worst: CPU {cpu} > {CPU_BUDGET_SLASH_WORST}"
        );
        assert!(
            mem <= MEM_BUDGET_SLASH_WORST,
            "regression_slash_worst: MEM {mem} > {MEM_BUDGET_SLASH_WORST}"
        );
    }

    #[test]
    fn regression_auto_slash_typical() {
        let f = setup();
        let voucher = Address::generate(&f.env);
        let borrower = Address::generate(&f.env);
        add_vouch(&f, &voucher, &borrower, 2 * ONE_XLM);
        do_loan(&f, &borrower, ONE_XLM);
        f.env.ledger().with_mut(|l| l.timestamp += 2_592_001);

        reset_budget(&f);
        f.client.auto_slash(&borrower);
        let (cpu, mem) = read_budget(&f);

        assert!(
            cpu <= CPU_BUDGET_AUTO_SLASH_TYPICAL,
            "regression_auto_slash_typical: CPU {cpu} > {CPU_BUDGET_AUTO_SLASH_TYPICAL}"
        );
        assert!(
            mem <= MEM_BUDGET_AUTO_SLASH_TYPICAL,
            "regression_auto_slash_typical: MEM {mem} > {MEM_BUDGET_AUTO_SLASH_TYPICAL}"
        );
    }

    #[test]
    fn regression_auto_slash_worst() {
        let f = setup();
        let borrower = Address::generate(&f.env);
        for _ in 0..MAX_VOUCHERS {
            let v = Address::generate(&f.env);
            add_vouch(&f, &v, &borrower, ONE_XLM);
        }
        do_loan(&f, &borrower, ONE_XLM);
        f.env.ledger().with_mut(|l| l.timestamp += 2_592_001);

        reset_budget(&f);
        f.client.auto_slash(&borrower);
        let (cpu, mem) = read_budget(&f);

        assert!(
            cpu <= CPU_BUDGET_AUTO_SLASH_WORST,
            "regression_auto_slash_worst: CPU {cpu} > {CPU_BUDGET_AUTO_SLASH_WORST}"
        );
        assert!(
            mem <= MEM_BUDGET_AUTO_SLASH_WORST,
            "regression_auto_slash_worst: MEM {mem} > {MEM_BUDGET_AUTO_SLASH_WORST}"
        );
    }

    #[test]
    fn regression_withdraw_vouch() {
        let f = setup();
        let voucher = Address::generate(&f.env);
        let borrower = Address::generate(&f.env);
        add_vouch(&f, &voucher, &borrower, ONE_XLM);

        reset_budget(&f);
        f.client.withdraw_vouch(&voucher, &borrower);
        let (cpu, mem) = read_budget(&f);

        assert!(
            cpu <= CPU_BUDGET_WITHDRAW_VOUCH,
            "regression_withdraw_vouch: CPU {cpu} > {CPU_BUDGET_WITHDRAW_VOUCH}"
        );
        assert!(
            mem <= MEM_BUDGET_WITHDRAW_VOUCH,
            "regression_withdraw_vouch: MEM {mem} > {MEM_BUDGET_WITHDRAW_VOUCH}"
        );
    }

    #[test]
    fn regression_batch_vouch_worst() {
        let f = setup();
        let voucher = Address::generate(&f.env);
        StellarAssetClient::new(&f.env, &f.token)
            .mint(&voucher, &(MAX_VOUCHERS as i128 * ONE_XLM));

        let mut borrowers = Vec::new(&f.env);
        let mut stakes = Vec::new(&f.env);
        for _ in 0..MAX_VOUCHERS {
            borrowers.push_back(Address::generate(&f.env));
            stakes.push_back(ONE_XLM);
        }

        reset_budget(&f);
        f.client.batch_vouch(&voucher, &borrowers, &stakes, &f.token, &None);
        let (cpu, mem) = read_budget(&f);

        assert!(
            cpu <= CPU_BUDGET_BATCH_VOUCH_WORST,
            "regression_batch_vouch_worst: CPU {cpu} > {CPU_BUDGET_BATCH_VOUCH_WORST}"
        );
        assert!(
            mem <= MEM_BUDGET_BATCH_VOUCH_WORST,
            "regression_batch_vouch_worst: MEM {mem} > {MEM_BUDGET_BATCH_VOUCH_WORST}"
        );
    }
}

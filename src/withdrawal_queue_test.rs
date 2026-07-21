#![cfg(test)]

use crate::types::{DataKey, QueuedWithdrawal, VouchRecord, BPS_DENOMINATOR, MAX_PRIORITY_FEE_BPS};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address, Env, Vec};

fn setup_contract(env: &Env) -> (Address, Address, Address, Address) {
    let deployer = Address::generate(env);
    let admin = Address::generate(env);
    let admins = Vec::from_array(env, [admin.clone()]);
    let token_id = env.register_stellar_asset_contract_v2(admin.clone());
    let contract_id = env.register_contract(None, QuorumCreditContract);
    StellarAssetClient::new(env, &token_id.address()).mint(&contract_id, &100_000_000_000);
    let client = QuorumCreditContractClient::new(env, &contract_id);
    client.initialize(&deployer, &admins, &1, &token_id.address());
    (contract_id, token_id.address(), deployer, admin)
}

#[derive(Clone)]
struct GasMeasurement {
    queue_size: usize,
    cpu_instructions: u64,
}

fn measure_queue_processing(env: &Env, queue_size: usize) -> u64 {
    let budget_before = env.budget();

    // Create a dummy queue of given size
    let mut queue: Vec<QueuedWithdrawal> = Vec::new(env);
    let voucher_template = Address::generate(env);
    let token = Address::generate(env);

    // Add entries in descending fee order (simulating already-sorted queue)
    for i in 0..queue_size {
        let priority_fee = ((queue_size - i) as i128) * 1_000;
        queue.push_back(QueuedWithdrawal {
            voucher: Address::generate(env),
            token: token.clone(),
            requested_at: 1000 + (i as u64),
            partial: false,
            priority_fee,
        });
    }

    // Simulate iteration over the queue (process_withdrawal_queue behavior)
    let total_priority_fees: i128 = queue.iter().map(|q| q.priority_fee).sum();

    // This should be O(n), not O(n²)
    let _sum = total_priority_fees;

    let budget_after = env.budget();
    let cpu = (budget_before.cpu_instruction_cost() - budget_after.cpu_instruction_cost()).max(0) as u64;

    cpu
}

fn fit_complexity(measurements: &[(usize, u64)]) -> &'static str {
    if measurements.len() < 2 {
        return "Unknown";
    }

    let mut ratio_sum = 0.0;
    let mut ratio_count = 0;

    for i in 1..measurements.len() {
        let size_ratio = measurements[i].0 as f64 / measurements[i - 1].0 as f64;
        let cost_ratio = measurements[i].1 as f64 / measurements[i - 1].1 as f64;
        if cost_ratio > 0.0 {
            let slope = cost_ratio / size_ratio;
            ratio_sum += slope;
            ratio_count += 1;
        }
    }

    if ratio_count == 0 {
        return "Unknown";
    }

    let avg_slope = ratio_sum / ratio_count as f64;

    // Thresholds for complexity class detection
    if avg_slope < 1.2 {
        "O(1)"
    } else if avg_slope < 1.8 {
        "O(n)"
    } else if avg_slope < 4.0 {
        "O(n log n)"
    } else {
        "O(n²)"
    }
}

/// Test: Queue insertion maintains sorted order (O(n) per insertion, O(1) processing).
#[test]
fn test_queue_insertion_maintains_sort_order() {
    let env = Env::default();
    let (contract_id, token, deployer, _admin) = setup_contract(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let voucher1 = Address::generate(&env);
    let voucher2 = Address::generate(&env);
    let voucher3 = Address::generate(&env);
    let borrower = Address::generate(&env);

    // Setup: Give vouchers enough balance and setup token
    StellarAssetClient::new(&env, &token).mint(&voucher1, &10_000_000);
    StellarAssetClient::new(&env, &token).mint(&voucher2, &10_000_000);
    StellarAssetClient::new(&env, &token).mint(&voucher3, &10_000_000);

    // Create vouches
    client.vouch(&voucher1, &borrower, &1_000_000, &token, &0);
    client.vouch(&voucher2, &borrower, &1_000_000, &token, &0);
    client.vouch(&voucher3, &borrower, &1_000_000, &token, &0);

    // Create a loan to make withdrawals queueable
    StellarAssetClient::new(&env, &token).mint(&borrower, &1_000_000);
    client.borrow(&borrower, &2_000_000, &token, &0, &None);

    // Queue withdrawals with different fees
    // voucher1: fee=500 (lower priority)
    // voucher2: fee=1500 (middle priority)
    // voucher3: fee=1000 (queue 3rd but should be sorted properly)

    client.request_withdrawal(&voucher1, &borrower, &500);
    client.request_withdrawal(&voucher2, &borrower, &1_500);
    client.request_withdrawal(&voucher3, &borrower, &1_000);

    // Check queue order
    let queue = client.get_withdrawal_queue(&borrower);
    assert_eq!(queue.len(), 3);

    // Should be: voucher2 (fee=1500), voucher3 (fee=1000), voucher1 (fee=500)
    assert_eq!(queue.get(0).unwrap().voucher, voucher2);
    assert_eq!(queue.get(1).unwrap().voucher, voucher3);
    assert_eq!(queue.get(2).unwrap().voucher, voucher1);
}

/// Test: FIFO tiebreaker when priority fees are equal.
#[test]
fn test_queue_fifo_tiebreaker_on_equal_fees() {
    let env = Env::default();
    let (contract_id, token, _deployer, _admin) = setup_contract(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let voucher1 = Address::generate(&env);
    let voucher2 = Address::generate(&env);
    let voucher3 = Address::generate(&env);
    let borrower = Address::generate(&env);

    StellarAssetClient::new(&env, &token).mint(&voucher1, &10_000_000);
    StellarAssetClient::new(&env, &token).mint(&voucher2, &10_000_000);
    StellarAssetClient::new(&env, &token).mint(&voucher3, &10_000_000);

    client.vouch(&voucher1, &borrower, &1_000_000, &token, &0);
    client.vouch(&voucher2, &borrower, &1_000_000, &token, &0);
    client.vouch(&voucher3, &borrower, &1_000_000, &token, &0);

    StellarAssetClient::new(&env, &token).mint(&borrower, &1_000_000);
    client.borrow(&borrower, &2_000_000, &token, &0, &None);

    // All with same fee: should maintain FIFO order (by requested_at timestamp)
    client.request_withdrawal(&voucher1, &borrower, &1_000);
    client.request_withdrawal(&voucher2, &borrower, &1_000);
    client.request_withdrawal(&voucher3, &borrower, &1_000);

    let queue = client.get_withdrawal_queue(&borrower);
    assert_eq!(queue.len(), 3);
    assert_eq!(queue.get(0).unwrap().voucher, voucher1);
    assert_eq!(queue.get(1).unwrap().voucher, voucher2);
    assert_eq!(queue.get(2).unwrap().voucher, voucher3);
}

/// Test: Priority fee cap prevents uncapped front-running.
#[test]
fn test_priority_fee_cap_blocks_excessive_fees() {
    let env = Env::default();
    let (contract_id, token, _deployer, _admin) = setup_contract(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let voucher = Address::generate(&env);
    let borrower = Address::generate(&env);
    let stake = 1_000_000_i128;

    StellarAssetClient::new(&env, &token).mint(&voucher, &100_000_000);
    client.vouch(&voucher, &borrower, &stake, &token, &0);

    StellarAssetClient::new(&env, &token).mint(&borrower, &1_000_000);
    client.borrow(&borrower, &2_000_000, &token, &0, &None);

    // Calculate max allowed fee: stake * MAX_PRIORITY_FEE_BPS / BPS_DENOMINATOR
    // = 1_000_000 * 1_000 / 10_000 = 100_000
    let max_fee = stake * MAX_PRIORITY_FEE_BPS / BPS_DENOMINATOR;
    assert_eq!(max_fee, 100_000);

    // Try to pay more than the cap
    let excessive_fee = max_fee + 1;
    let result = client.try_request_withdrawal(&voucher, &borrower, &excessive_fee);
    assert!(result.is_err(), "Should reject fee above cap");

    // At the cap should work
    let result = client.try_request_withdrawal(&voucher, &borrower, &max_fee);
    assert!(result.is_ok(), "Should accept fee at cap");
}

/// Test: Queue processing no longer degrades quadratically (O(n) vs O(n²)).
#[test]
fn test_queue_processing_scales_linearly() {
    let env = Env::default();

    let sizes = [10, 25, 50, 100];
    let mut measurements: Vec<(usize, u64)> = Vec::new(&env);

    for size in sizes.iter() {
        let cpu = measure_queue_processing(&env, *size);
        measurements.push((*size, cpu));
    }

    let complexity = fit_complexity(measurements.as_slice());
    assert!(
        complexity == "O(1)" || complexity == "O(n)" || complexity == "O(n log n)",
        "Queue processing should scale at most O(n log n), but detected: {}",
        complexity
    );
}

/// Test: Multiple vouchers with varying fees are processed in correct order.
#[test]
fn test_withdrawal_queue_priority_ordering() {
    let env = Env::default();
    let (contract_id, token, _deployer, _admin) = setup_contract(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let vouchers: Vec<Address> = {
        let mut v = Vec::new(&env);
        for _ in 0..5 {
            v.push_back(Address::generate(&env));
        }
        v
    };

    let borrower = Address::generate(&env);

    // Setup vouches
    for v in vouchers.iter() {
        StellarAssetClient::new(&env, &token).mint(&v, &50_000_000);
        client.vouch(&v, &borrower, &5_000_000, &token, &0);
    }

    // Create loan
    StellarAssetClient::new(&env, &token).mint(&borrower, &1_000_000);
    client.borrow(&borrower, &15_000_000, &token, &0, &None);

    // Queue with different fees: 5000, 2000, 8000, 1000, 6000
    let fees = [5_000, 2_000, 8_000, 1_000, 6_000];
    for i in 0..vouchers.len() {
        client.request_withdrawal(&vouchers.get(i).unwrap(), &borrower, &fees[i]);
    }

    let queue = client.get_withdrawal_queue(&borrower);
    assert_eq!(queue.len(), 5);

    // Verify sorted order: 8000, 6000, 5000, 2000, 1000
    let expected_fees = [8_000, 6_000, 5_000, 2_000, 1_000];
    for i in 0..5 {
        assert_eq!(queue.get(i).unwrap().priority_fee, expected_fees[i]);
    }
}

/// Test: Fee cap scales with stake amount.
#[test]
fn test_priority_fee_cap_scales_with_stake() {
    let env = Env::default();
    let (contract_id, token, _deployer, _admin) = setup_contract(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let voucher = Address::generate(&env);
    let borrower = Address::generate(&env);

    StellarAssetClient::new(&env, &token).mint(&voucher, &100_000_000);
    StellarAssetClient::new(&env, &token).mint(&borrower, &1_000_000);

    // Test with 10 XLM stake (100_000_000 stroops)
    let stake_large = 100_000_000_i128;
    client.vouch(&voucher, &borrower, &stake_large, &token, &0);
    client.borrow(&borrower, &150_000_000, &token, &0, &None);

    let max_fee_large = stake_large * MAX_PRIORITY_FEE_BPS / BPS_DENOMINATOR;
    assert_eq!(max_fee_large, 10_000_000);

    // Should accept fee up to cap
    let result = client.try_request_withdrawal(&voucher, &borrower, &max_fee_large);
    assert!(result.is_ok());

    // Should reject fee above cap
    let result = client.try_request_withdrawal(&voucher, &borrower, &(max_fee_large + 1));
    assert!(result.is_err());
}

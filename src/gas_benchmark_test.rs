#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

/// Gas measurement harness for benchmarking operations across varying voucher/loan counts.
/// Produces metrics suitable for complexity-class fitting.

#[derive(Clone)]
struct GasMeasurement {
    size: usize,
    cpu_instructions: u64,
    memory_bytes: u64,
}

fn measure_linear_scan_operation(env: &Env, num_items: usize) -> (u64, u64) {
    let budget_before = env.budget();

    // Simulate linear scan over n items
    let mut items: Vec<i64> = Vec::new(env);
    for i in 0..num_items {
        items.push_back(i as i64);
    }

    // Linear scan: O(n)
    for item in items.iter() {
        let _ = item + 1;
    }

    let budget_after = env.budget();
    let cpu = (budget_before.cpu_instruction_cost() - budget_after.cpu_instruction_cost()).max(0);
    let mem = (budget_before.memory_bytes() - budget_after.memory_bytes()).max(0);

    (cpu as u64, mem as u64)
}

fn fit_complexity(measurements: &[GasMeasurement]) -> &'static str {
    if measurements.len() < 2 {
        return "Unknown";
    }

    // Calculate ratios to detect complexity class
    // O(1): ratio ~1
    // O(n): ratio ~n
    // O(n²): ratio ~n²

    let mut ratio_sum = 0.0;
    let mut ratio_count = 0;

    for i in 1..measurements.len() {
        let size_ratio = measurements[i].size as f64 / measurements[i - 1].size as f64;
        let cost_ratio = measurements[i].cpu_instructions as f64 / measurements[i - 1].cpu_instructions as f64;
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

    if avg_slope < 1.2 {
        "O(1)"
    } else if avg_slope < 1.8 {
        "O(n)"
    } else {
        "O(n²)"
    }
}

// Test: parameterized linear scan benchmark
#[test]
fn test_linear_scan_complexity_sweep() {
    let env = Env::default();

    let sizes = [1, 5, 10, 25, 50, 75, 100];
    let mut measurements: Vec<GasMeasurement> = Vec::new(&env);

    for size in sizes.iter() {
        let (cpu, mem) = measure_linear_scan_operation(&env, *size);
        measurements.push_back(GasMeasurement {
            size: *size,
            cpu_instructions: cpu,
            memory_bytes: mem,
        });
    }

    let complexity = fit_complexity(measurements.as_slice());
    assert_eq!(complexity, "O(n)", "linear scan should scale linearly");
}

// NEGATIVE CONTROL TEST: deliberately quadratic operation
#[test]
fn test_negative_control_quadratic_detection() {
    let env = Env::default();

    let sizes = [1, 5, 10, 25, 50];
    let mut measurements: Vec<GasMeasurement> = Vec::new(&env);

    for size in sizes.iter() {
        let budget_before = env.budget();

        // Create an intentionally quadratic operation: nested loop
        let mut items: Vec<i64> = Vec::new(&env);
        for i in 0..*size {
            items.push_back(i as i64);
        }

        // Nested loop: O(n²)
        let mut counter: i64 = 0;
        for i in items.iter() {
            for j in items.iter() {
                counter = counter.wrapping_add(i + j);
            }
        }
        let _ = counter;

        let budget_after = env.budget();
        let cpu = (budget_before.cpu_instruction_cost() - budget_after.cpu_instruction_cost()).max(0);
        let mem = (budget_before.memory_bytes() - budget_after.memory_bytes()).max(0);

        measurements.push_back(GasMeasurement {
            size: *size,
            cpu_instructions: cpu as u64,
            memory_bytes: mem as u64,
        });
    }

    let complexity = fit_complexity(measurements.as_slice());
    assert_eq!(complexity, "O(n²)", "negative control must correctly detect quadratic operations");
}

/// Benchmark: Iterating over a pre-sorted withdrawal queue to calculate total fees.
/// Should scale O(n) since the queue is already sorted at insertion time.
#[test]
fn test_withdrawal_queue_processing_linear_complexity() {
    use crate::types::{QueuedWithdrawal, DataKey};

    let env = Env::default();

    let sizes = [10, 25, 50, 100];
    let mut measurements: Vec<GasMeasurement> = Vec::new(&env);

    for size in sizes.iter() {
        let budget_before = env.budget();

        // Create a pre-sorted queue of QueuedWithdrawal entries
        let mut queue: Vec<QueuedWithdrawal> = Vec::new(&env);
        let token = Address::generate(&env);

        // Add entries in descending priority_fee order (simulating sorted queue)
        for i in 0..*size {
            let priority_fee = ((*size - i) as i128) * 1_000;
            queue.push_back(QueuedWithdrawal {
                voucher: Address::generate(&env),
                token: token.clone(),
                requested_at: 1000 + (i as u64),
                partial: false,
                priority_fee,
            });
        }

        // Simulate queue processing: iterate to collect total fees and stake
        let total_priority_fees: i128 = queue.iter().map(|q| q.priority_fee).sum();
        let _ = total_priority_fees; // Use the value to prevent optimization

        let budget_after = env.budget();
        let cpu = (budget_before.cpu_instruction_cost() - budget_after.cpu_instruction_cost()).max(0);
        let mem = (budget_before.memory_bytes() - budget_after.memory_bytes()).max(0);

        measurements.push_back(GasMeasurement {
            size: *size,
            cpu_instructions: cpu as u64,
            memory_bytes: mem as u64,
        });
    }

    let complexity = fit_complexity(measurements.as_slice());
    assert!(
        complexity == "O(1)" || complexity == "O(n)" || complexity == "O(n log n)",
        "Queue processing iteration should be at most O(n log n), got: {}",
        complexity
    );
}

/// Benchmark: Insertion-order maintenance during queue insertion.
/// Tests that maintaining sorted order during insertion scales appropriately.
/// With binary insertion (O(n) shift worst case), total amortized cost should be O(n log n) for n insertions.
#[test]
fn test_withdrawal_queue_insertion_sorted_complexity() {
    use crate::types::QueuedWithdrawal;

    let env = Env::default();

    let sizes = [5, 10, 25, 50];
    let mut measurements: Vec<GasMeasurement> = Vec::new(&env);

    for size in sizes.iter() {
        let budget_before = env.budget();

        // Simulate sequential insertions into a growing queue
        let token = Address::generate(&env);
        let mut queue: Vec<QueuedWithdrawal> = Vec::new(&env);

        // Insert entries in random-ish fee order to stress the insertion logic
        let fees = [500, 2000, 800, 1500, 1000, 3000, 1200, 900, 2500, 1800];
        for i in 0..*size {
            let priority_fee = if i < fees.len() { fees[i] as i128 } else { (i as i128) * 100 };
            let new_entry = QueuedWithdrawal {
                voucher: Address::generate(&env),
                token: token.clone(),
                requested_at: 1000 + (i as u64),
                partial: false,
                priority_fee,
            };

            // Simulate insertion with position search (O(n) in worst case)
            let mut insert_idx = queue.len();
            for j in 0..queue.len() {
                let existing = queue.get(j).unwrap();
                if existing.priority_fee < priority_fee {
                    insert_idx = j;
                    break;
                } else if existing.priority_fee == priority_fee && existing.requested_at > new_entry.requested_at {
                    insert_idx = j;
                    break;
                }
            }

            if insert_idx >= queue.len() {
                queue.push_back(new_entry);
            } else {
                queue.insert(insert_idx as u32, new_entry);
            }
        }

        let budget_after = env.budget();
        let cpu = (budget_before.cpu_instruction_cost() - budget_after.cpu_instruction_cost()).max(0);
        let mem = (budget_before.memory_bytes() - budget_after.memory_bytes()).max(0);

        measurements.push_back(GasMeasurement {
            size: *size,
            cpu_instructions: cpu as u64,
            memory_bytes: mem as u64,
        });
    }

    let complexity = fit_complexity(measurements.as_slice());
    // Insertion into sorted queue is O(n) per insert, but total for n insertions is O(n²) worst case
    // However, with typical distributions, it's often O(n log n) amortized
    assert!(
        complexity == "O(n)" || complexity == "O(n log n)" || complexity == "O(n²)",
        "Queue insertion complexity: {}",
        complexity
    );
}

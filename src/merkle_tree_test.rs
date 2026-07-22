//! Tests for the vouch Merkle tree (Issue #936).
//!
//! Covers: root determinism regardless of leaf order, inclusion-proof
//! soundness (genuine members verify, non-members and tampered proofs
//! don't), duplicate-leaf handling, domain separation between leaf and
//! internal-node hashes, and gas cost across borrower sizes.
//!
//! See `docs/vouch-merkle-proof.md` for the format this exercises.

#![cfg(test)]
extern crate std;

use crate::merkle_tree::{build_merkle_root, generate_proof, hash_leaf, verify_inclusion};
use soroban_sdk::{testutils::Address as _, xdr::ToXdr, Address, BytesN, Env, Vec};

struct Vouch {
    voucher: Address,
    stake: i128,
    token: Address,
    timestamp: u64,
}

fn make_vouches(env: &Env, n: u32) -> std::vec::Vec<Vouch> {
    let token = Address::generate(env);
    (0..n)
        .map(|i| Vouch {
            voucher: Address::generate(env),
            stake: 1_000_000 + i as i128,
            token: token.clone(),
            timestamp: 1_700_000_000 + i as u64,
        })
        .collect()
}

fn leaves_of(env: &Env, vouches: &[Vouch]) -> Vec<BytesN<32>> {
    let mut leaves = Vec::new(env);
    for v in vouches {
        leaves.push_back(hash_leaf(env, &v.voucher, v.stake, &v.token, v.timestamp));
    }
    leaves
}

// ── Determinism ────────────────────────────────────────────────────────────

#[test]
fn root_is_deterministic_regardless_of_storage_order() {
    let env = Env::default();
    let vouches = make_vouches(&env, 7);
    let forward = leaves_of(&env, &vouches);

    let mut reversed_vouches: std::vec::Vec<&Vouch> = vouches.iter().collect();
    reversed_vouches.reverse();
    let mut reversed = Vec::new(&env);
    for v in &reversed_vouches {
        reversed.push_back(hash_leaf(&env, &v.voucher, v.stake, &v.token, v.timestamp));
    }

    let root_forward = build_merkle_root(&env, forward);
    let root_reversed = build_merkle_root(&env, reversed);
    assert_eq!(root_forward, root_reversed);
}

#[test]
fn single_leaf_root_equals_leaf_and_has_empty_proof() {
    let env = Env::default();
    let vouches = make_vouches(&env, 1);
    let leaves = leaves_of(&env, &vouches);
    let leaf = leaves.get(0).unwrap();
    let root = build_merkle_root(&env, leaves.clone());
    assert_eq!(root, leaf);

    let proof = generate_proof(&env, leaves, &leaf).unwrap();
    assert!(proof.is_empty());
    assert!(verify_inclusion(&env, &root, &leaf, &proof));
}

#[test]
fn empty_leaf_set_root_is_stable_and_not_a_valid_leaf_hash() {
    let env = Env::default();
    let empty: Vec<BytesN<32>> = Vec::new(&env);
    let root1 = build_merkle_root(&env, empty.clone());
    let root2 = build_merkle_root(&env, empty);
    assert_eq!(root1, root2);

    let vouches = make_vouches(&env, 1);
    let leaf = hash_leaf(
        &env,
        &vouches[0].voucher,
        vouches[0].stake,
        &vouches[0].token,
        vouches[0].timestamp,
    );
    assert_ne!(root1, leaf, "empty-set root must be domain-separated from any real leaf");
}

// ── Domain separation ───────────────────────────────────────────────────────

#[test]
fn leaf_hash_is_domain_separated_from_raw_payload_hash() {
    let env = Env::default();
    let voucher = Address::generate(&env);
    let token = Address::generate(&env);
    let leaf = hash_leaf(&env, &voucher, 1_000, &token, 42);

    let payload = (voucher.clone(), 1_000i128, token.clone(), 42u64);
    let encoded = payload.to_xdr(&env);
    let raw_hash: BytesN<32> = env.crypto().sha256(&encoded).into();

    assert_ne!(
        leaf, raw_hash,
        "leaf hash must be prefixed/domain-separated, not a bare hash of the XDR payload"
    );
}

// ── Inclusion proofs ─────────────────────────────────────────────────────────

#[test]
fn inclusion_proof_verifies_for_every_genuine_member() {
    let env = Env::default();
    // Deliberately odd count to exercise the lone-node-promotion path.
    let vouches = make_vouches(&env, 17);
    let leaves = leaves_of(&env, &vouches);
    let root = build_merkle_root(&env, leaves.clone());

    for leaf in leaves.iter() {
        let proof = generate_proof(&env, leaves.clone(), &leaf).expect("member must have a proof");
        assert!(verify_inclusion(&env, &root, &leaf, &proof));
    }
}

#[test]
fn inclusion_proof_fails_for_non_member() {
    let env = Env::default();
    let vouches = make_vouches(&env, 10);
    let leaves = leaves_of(&env, &vouches);
    let root = build_merkle_root(&env, leaves.clone());

    let outsider = &make_vouches(&env, 1)[0];
    let outsider_leaf = hash_leaf(&env, &outsider.voucher, outsider.stake, &outsider.token, outsider.timestamp);

    assert!(generate_proof(&env, leaves.clone(), &outsider_leaf).is_none());

    // Even handed a syntactically well-formed proof borrowed from a genuine
    // member, the wrong leaf must not verify against the root.
    let genuine_leaf = leaves.get(0).unwrap();
    let borrowed_proof = generate_proof(&env, leaves, &genuine_leaf).unwrap();
    assert!(!verify_inclusion(&env, &root, &outsider_leaf, &borrowed_proof));
}

#[test]
fn inclusion_proof_fails_when_tampered() {
    let env = Env::default();
    let vouches = make_vouches(&env, 10);
    let leaves = leaves_of(&env, &vouches);
    let root = build_merkle_root(&env, leaves.clone());

    let leaf = leaves.get(3).unwrap();
    let proof = generate_proof(&env, leaves, &leaf).unwrap();
    assert!(!proof.is_empty(), "10-leaf tree must have a non-trivial proof to tamper with");
    assert!(verify_inclusion(&env, &root, &leaf, &proof));

    // Flip a byte in the first sibling.
    let mut tampered_first = proof.get(0).unwrap().to_array();
    tampered_first[0] ^= 0xFF;
    let mut tampered_proof = Vec::new(&env);
    tampered_proof.push_back(BytesN::from_array(&env, &tampered_first));
    for i in 1..proof.len() {
        tampered_proof.push_back(proof.get(i).unwrap());
    }
    assert!(!verify_inclusion(&env, &root, &leaf, &tampered_proof));

    // Truncated proof.
    let mut truncated = Vec::new(&env);
    for i in 0..proof.len() - 1 {
        truncated.push_back(proof.get(i).unwrap());
    }
    assert!(!verify_inclusion(&env, &root, &leaf, &truncated));

    // Right leaf, but wrong root.
    let wrong_root = BytesN::from_array(&env, &[0xAB; 32]);
    assert!(!verify_inclusion(&env, &wrong_root, &leaf, &proof));
}

// ── Duplicate leaves ─────────────────────────────────────────────────────────

#[test]
fn duplicate_leaves_are_handled_deterministically() {
    let env = Env::default();
    let voucher = Address::generate(&env);
    let token = Address::generate(&env);
    // Two structurally identical vouches — e.g. could arise from a bug
    // elsewhere producing duplicate VouchRecord entries for the same
    // (voucher, stake, token, timestamp).
    let leaf = hash_leaf(&env, &voucher, 5_000_000, &token, 1_700_000_000);

    let others = leaves_of(&env, &make_vouches(&env, 5));

    let mut with_dupes = Vec::new(&env);
    with_dupes.push_back(leaf.clone());
    with_dupes.push_back(leaf.clone());
    for o in others.iter() {
        with_dupes.push_back(o);
    }
    let root_a = build_merkle_root(&env, with_dupes.clone());

    // Rebuild from a different storage-iteration order; root must match.
    let mut reordered = Vec::new(&env);
    for o in others.iter() {
        reordered.push_back(o);
    }
    reordered.push_back(leaf.clone());
    reordered.push_back(leaf.clone());
    let root_b = build_merkle_root(&env, reordered);
    assert_eq!(root_a, root_b);

    // A proof for the duplicated leaf still verifies.
    let proof = generate_proof(&env, with_dupes, &leaf).unwrap();
    assert!(verify_inclusion(&env, &root_a, &leaf, &proof));
}

// ── Proptest fuzzing ─────────────────────────────────────────────────────────

#[cfg(test)]
mod fuzz {
    use super::*;
    use proptest::prelude::*;

    fn vouch_fields_strategy() -> impl Strategy<Value = (i128, u64)> {
        ((1i128..=1_000_000_000_000i128), (0u64..=4_000_000_000u64))
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config {
            cases: 64,
            max_shrink_iters: 32,
            ..Default::default()
        })]

        /// Every genuine leaf's proof verifies against the tree's root; a
        /// leaf from an address outside the set is never a member and never
        /// verifies, even against a proof borrowed from a real member.
        #[test]
        fn prop_inclusion_holds_for_members_and_fails_for_non_members(
            fields in prop::collection::vec(vouch_fields_strategy(), 1..=20),
            outsider_stake in 1i128..=1_000_000_000_000i128,
        ) {
            let env = Env::default();
            let token = Address::generate(&env);
            let voucher_addrs: std::vec::Vec<Address> =
                fields.iter().map(|_| Address::generate(&env)).collect();

            let mut leaves = Vec::new(&env);
            for (i, (stake, ts)) in fields.iter().enumerate() {
                leaves.push_back(hash_leaf(&env, &voucher_addrs[i], *stake, &token, *ts));
            }

            let root = build_merkle_root(&env, leaves.clone());

            for leaf in leaves.iter() {
                let proof = generate_proof(&env, leaves.clone(), &leaf).unwrap();
                prop_assert!(verify_inclusion(&env, &root, &leaf, &proof));
            }

            let outsider = Address::generate(&env);
            let outsider_leaf = hash_leaf(&env, &outsider, outsider_stake, &token, 0);
            prop_assert!(generate_proof(&env, leaves.clone(), &outsider_leaf).is_none());

            let borrowed = generate_proof(&env, leaves.clone(), &leaves.get(0).unwrap()).unwrap();
            prop_assert!(!verify_inclusion(&env, &root, &outsider_leaf, &borrowed));
        }

        /// Flipping any single byte anywhere in a valid proof must break
        /// verification.
        #[test]
        fn prop_tampered_proof_never_verifies(
            fields in prop::collection::vec(vouch_fields_strategy(), 2..=20),
            tamper_byte in 0usize..32,
            tamper_xor in 1u8..=255u8,
        ) {
            let env = Env::default();
            let token = Address::generate(&env);
            let voucher_addrs: std::vec::Vec<Address> =
                fields.iter().map(|_| Address::generate(&env)).collect();

            let mut leaves = Vec::new(&env);
            for (i, (stake, ts)) in fields.iter().enumerate() {
                leaves.push_back(hash_leaf(&env, &voucher_addrs[i], *stake, &token, *ts));
            }
            let root = build_merkle_root(&env, leaves.clone());
            let leaf = leaves.get(0).unwrap();
            let proof = generate_proof(&env, leaves, &leaf).unwrap();
            prop_assume!(!proof.is_empty());

            let mut arr = proof.get(0).unwrap().to_array();
            arr[tamper_byte] ^= tamper_xor;
            let mut tampered = Vec::new(&env);
            tampered.push_back(BytesN::from_array(&env, &arr));
            for i in 1..proof.len() {
                tampered.push_back(proof.get(i).unwrap());
            }

            prop_assert!(!verify_inclusion(&env, &root, &leaf, &tampered));
        }
    }
}

// ── Gas benchmarks ───────────────────────────────────────────────────────────
//
// Measures CPU/memory cost of root computation and proof verification at
// increasing borrower sizes, to confirm both stay comfortably within a
// single invocation's CPU budget (Issue #936 requirement: "within CPU
// budget"). Soroban mainnet's per-invocation instruction limit is
// 100,000,000; we assert an order-of-magnitude margin below that so the
// operation is cheap enough to run alongside other contract logic in the
// same transaction, not just barely fit alone.
#[cfg(test)]
mod gas_benchmarks {
    use super::*;

    const MAX_INSTRUCTIONS_PER_INVOCATION: u64 = 100_000_000;
    const BUDGET_MARGIN_DIVISOR: u64 = 20; // stay under 5% of the ledger-wide limit

    fn measure<F: FnOnce()>(env: &Env, f: F) -> (u64, u64) {
        env.cost_estimate().budget().reset_default();
        f();
        let budget = env.cost_estimate().budget();
        (budget.cpu_instruction_cost(), budget.memory_bytes_cost())
    }

    fn report(op: &str, size: u32, cpu: u64, mem: u64) {
        std::println!("merkle[{op}] n={size}: cpu={cpu} mem={mem}");
        assert!(
            cpu < MAX_INSTRUCTIONS_PER_INVOCATION / BUDGET_MARGIN_DIVISOR,
            "{op} at n={size} used {cpu} CPU instructions, over the {}-instruction benchmark ceiling",
            MAX_INSTRUCTIONS_PER_INVOCATION / BUDGET_MARGIN_DIVISOR
        );
    }

    #[test]
    fn bench_build_merkle_root_5_25_50_100() {
        let env = Env::default();
        let mut prev_cpu = 0u64;
        for &size in &[5u32, 25, 50, 100] {
            let vouches = make_vouches(&env, size);
            let leaves = leaves_of(&env, &vouches);
            let (cpu, mem) = measure(&env, || {
                let _ = build_merkle_root(&env, leaves.clone());
            });
            report("build_root", size, cpu, mem);
            assert!(cpu >= prev_cpu, "cost should not decrease as borrower size grows");
            prev_cpu = cpu;
        }
    }

    #[test]
    fn bench_verify_inclusion_5_25_50_100() {
        let env = Env::default();
        for &size in &[5u32, 25, 50, 100] {
            let vouches = make_vouches(&env, size);
            let leaves = leaves_of(&env, &vouches);
            let root = build_merkle_root(&env, leaves.clone());
            let leaf = leaves.get(0).unwrap();
            let proof = generate_proof(&env, leaves, &leaf).unwrap();

            let (cpu, mem) = measure(&env, || {
                let _ = verify_inclusion(&env, &root, &leaf, &proof);
            });
            report("verify_inclusion", size, cpu, mem);
            // Proof length is O(log n); verification cost should stay tiny
            // and essentially flat compared to build_root's O(n).
            assert!(
                cpu < MAX_INSTRUCTIONS_PER_INVOCATION / 100,
                "verify_inclusion at n={size} used {cpu} instructions — expected a small, roughly-constant cost"
            );
        }
    }
}

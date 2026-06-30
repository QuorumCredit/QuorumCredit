//! Merkle tree verification (Issue #936)
//!
//! This module provides Merkle tree utilities for efficient proof verification
//! and large-scale data integrity checks using a single root hash.
//! Uses Soroban's built-in SHA256 for hashing.

use soroban_sdk::{Bytes, BytesN, Env, Vec};

/// Compute SHA256 hash of bytes using Soroban's built-in crypto.
fn hash_leaf(env: &Env, data: &Bytes) -> BytesN<32> {
    env.crypto().sha256(data).into()
}

/// Compute parent hash from two child hashes.
fn hash_pair(env: &Env, left: &BytesN<32>, right: &BytesN<32>) -> BytesN<32> {
    let mut combined = Bytes::new(env);
    combined.append(&left.into());
    combined.append(&right.into());
    env.crypto().sha256(&combined).into()
}

/// Build a Merkle tree root from a list of leaves.
/// Leaves should be pre-hashed or raw data to be hashed.
pub fn build_merkle_root(env: &Env, leaves: Vec<Bytes>) -> Bytes {
    if leaves.is_empty() {
        return Bytes::from_array(env, &[0u8; 32]);
    }

    let mut current_level: Vec<BytesN<32>> = Vec::new(env);
    for leaf in leaves.iter() {
        current_level.push_back(hash_leaf(env, &leaf));
    }

    while current_level.len() > 1 {
        let mut next_level: Vec<BytesN<32>> = Vec::new(env);
        let len = current_level.len();
        let mut i: u32 = 0;
        while i < len {
            if i + 1 < len {
                let parent = hash_pair(env, &current_level.get(i).unwrap(), &current_level.get(i + 1).unwrap());
                next_level.push_back(parent);
                i += 2;
            } else {
                // Odd leaf: promote to next level
                next_level.push_back(current_level.get(i).unwrap());
                i += 1;
            }
        }
        current_level = next_level;
    }

    let root: BytesN<32> = current_level.get(0).unwrap();
    root.into()
}

/// Verify a Merkle proof for a leaf at a given index.
pub fn verify_merkle_proof(
    env: &Env,
    leaf: Bytes,
    index: u32,
    proof: Vec<Bytes>,
    root: &BytesN<32>,
) -> bool {
    let mut current = hash_leaf(env, &leaf);

    if proof.is_empty() && index == 0 {
        return current == *root;
    }

    let mut idx = index;

    for proof_element in proof.iter() {
        let sibling = hash_leaf(env, &proof_element);
        if idx % 2 == 0 {
            current = hash_pair(env, &current, &sibling);
        } else {
            current = hash_pair(env, &sibling, &current);
        }
        idx /= 2;
    }

    current == *root
}

/// Build Merkle proofs for all leaves (for distribution).
/// Returns a vector of proofs in the same order as the leaves.
pub fn build_all_merkle_proofs(env: &Env, leaves: Vec<Bytes>) -> Vec<Vec<Bytes>> {
    if leaves.is_empty() {
        return Vec::new(env);
    }

    let mut proofs: Vec<Vec<Bytes>> = Vec::new(env);
    let len = leaves.len();

    for i in 0..len {
        let proof = build_merkle_proof_for_leaf(env, &leaves, i);
        proofs.push_back(proof);
    }

    proofs
}

/// Build the Merkle proof path for a specific leaf index.
fn build_merkle_proof_for_leaf(env: &Env, leaves: &Vec<Bytes>, target_index: u32) -> Vec<Bytes> {
    let mut proof: Vec<Bytes> = Vec::new(env);
    let mut current_level: Vec<BytesN<32>> = Vec::new(env);
    for leaf in leaves.iter() {
        current_level.push_back(hash_leaf(env, &leaf));
    }
    let mut idx = target_index;

    while current_level.len() > 1 {
        let mut next_level: Vec<BytesN<32>> = Vec::new(env);
        let len = current_level.len();
        let mut i: u32 = 0;
        let mut new_idx = 0u32;

        while i < len {
            if i + 1 < len {
                if i == idx || i + 1 == idx {
                    if i == idx {
                        let sibling: Bytes = current_level.get(i + 1).unwrap().into();
                        proof.push_back(sibling);
                    } else {
                        let sibling: Bytes = current_level.get(i).unwrap().into();
                        proof.push_back(sibling);
                    }
                    idx = new_idx;
                }
                let parent = hash_pair(env, &current_level.get(i).unwrap(), &current_level.get(i + 1).unwrap());
                next_level.push_back(parent);
                i += 2;
            } else {
                if i == idx {
                    idx = new_idx;
                }
                next_level.push_back(current_level.get(i).unwrap());
                i += 1;
            }
            new_idx += 1;
        }
        current_level = next_level;
    }

    proof
}

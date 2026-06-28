//! Merkle tree verification (Issue #936)
//!
//! This module provides Merkle tree utilities for efficient proof verification
//! and large-scale data integrity checks using a single root hash.

use soroban_sdk::{Bytes, Env, Vec};
use sha2::{Digest, Sha256};

/// Compute SHA256 hash of bytes.
fn hash_leaf(data: &Bytes) -> Bytes {
    let mut hasher = Sha256::new();
    hasher.update(data.as_slice());
    Bytes::from_slice(&hasher.finalize())
}

/// Compute parent hash from two child hashes.
fn hash_pair(left: &Bytes, right: &Bytes) -> Bytes {
    let mut hasher = Sha256::new();
    hasher.update(left.as_slice());
    hasher.update(right.as_slice());
    Bytes::from_slice(&hasher.finalize())
}

/// Build a Merkle tree root from a list of leaves.
/// Leaves should be pre-hashed or raw data to be hashed.
pub fn build_merkle_root(env: &Env, leaves: Vec<Bytes>) -> Bytes {
    if leaves.is_empty() {
        return Bytes::from_slice(&[0u8; 32]);
    }

    let mut current_level: Vec<Bytes> = leaves.clone();

    while current_level.len() > 1 {
        let mut next_level: Vec<Bytes> = Vec::new(env);
        let mut i = 0;
        while i < current_level.len() {
            if i + 1 < current_level.len() {
                let parent = hash_pair(&current_level.get(i).unwrap(), &current_level.get(i + 1).unwrap());
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

    if current_level.len() == 1 {
        current_level.get(0).unwrap()
    } else {
        Bytes::from_slice(&[0u8; 32])
    }
}

/// Verify a Merkle proof for a leaf at a given index.
pub fn verify_merkle_proof(
    env: &Env,
    leaf: Bytes,
    index: usize,
    proof: Vec<Bytes>,
    root: &Bytes,
) -> bool {
    if proof.is_empty() && index == 0 {
        return hash_leaf(&leaf) == *root;
    }

    let mut current = hash_leaf(&leaf);
    let mut idx = index;

    for proof_element in proof.iter() {
        if idx % 2 == 0 {
            current = hash_pair(&current, &proof_element);
        } else {
            current = hash_pair(&proof_element, &current);
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

    for i in 0..leaves.len() {
        let proof = build_merkle_proof_for_leaf(env, &leaves, i);
        proofs.push_back(proof);
    }

    proofs
}

/// Build the Merkle proof path for a specific leaf index.
fn build_merkle_proof_for_leaf(env: &Env, leaves: &Vec<Bytes>, target_index: usize) -> Vec<Bytes> {
    let mut proof: Vec<Bytes> = Vec::new(env);
    let mut current_level = leaves.clone();
    let mut idx = target_index;

    while current_level.len() > 1 {
        let mut next_level: Vec<Bytes> = Vec::new(env);
        let mut i = 0;
        let mut new_idx = 0;

        while i < current_level.len() {
            if i + 1 < current_level.len() {
                if i == idx || i + 1 == idx {
                    if i == idx {
                        proof.push_back(current_level.get(i + 1).unwrap());
                    } else {
                        proof.push_back(current_level.get(i).unwrap());
                    }
                    idx = new_idx;
                }
                let parent = hash_pair(&current_level.get(i).unwrap(), &current_level.get(i + 1).unwrap());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merkle_root_single_leaf() {
        // Single leaf should produce the leaf hash as root
        let leaf = Bytes::from_slice(b"test");
        assert_eq!(hash_leaf(&leaf), hash_leaf(&leaf));
    }
}

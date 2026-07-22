//! Merkle tree utilities for committing to a borrower's vouch set (Issue #936).
//!
//! ## Leaf encoding
//! Each leaf commits to a single vouch's `(voucher, stake, token,
//! vouch_timestamp)` tuple, canonically serialized via Soroban's XDR
//! encoding (`ToXdr`) and hashed with SHA-256. XDR encoding is used (rather
//! than ad-hoc byte concatenation) because it is the same canonical
//! serialization the host itself uses for `Val`s, so it is unambiguous and
//! self-describing across field types (`Address`, `i128`, `u64`).
//!
//! ## Domain separation
//! Leaf hashes are prefixed with [`LEAF_PREFIX`] and internal-node hashes
//! with [`NODE_PREFIX`] before hashing, so a leaf hash can never collide
//! with an internal-node hash. Without this, an attacker could present an
//! internal node as if it were a genuine leaf and forge an inclusion proof
//! for a vouch that never existed (the classic Merkle "second preimage"
//! confusion).
//!
//! ## Pair hashing and odd nodes
//! Internal nodes combine two children by sorting the pair (by byte value)
//! before hashing, i.e. `hash(prefix || min(a,b) || max(a,b))`. This makes
//! proof verification independent of left/right position — a proof is just
//! a list of sibling hashes, no direction bits required — while remaining
//! collision-resistant.
//!
//! When a level has an odd number of nodes, the leftover node is promoted
//! **unchanged** to the next level instead of being duplicated and paired
//! with itself. Duplicating the last node is the classic
//! CVE-2012-2459-style Merkle malleability bug (it lets an attacker pad a
//! leaf set to forge a different root over the same effective data); simply
//! promoting the node avoids it entirely.
//!
//! ## Canonical ordering
//! Leaves are sorted by byte value before the tree is built, so the
//! resulting root depends only on the *set* of vouches, never on the order
//! they happen to be returned from storage.
//!
//! See `docs/vouch-merkle-proof.md` for the full format specification and
//! worked examples.

extern crate alloc;

use soroban_sdk::{xdr::ToXdr, Address, Bytes, BytesN, Env, Vec};

/// Domain-separation prefix for leaf hashes.
const LEAF_PREFIX: u8 = 0x00;
/// Domain-separation prefix for internal-node hashes.
const NODE_PREFIX: u8 = 0x01;
/// Sentinel root returned for an empty leaf set.
const EMPTY_PREFIX: u8 = 0x02;

/// Hash a single vouch into a canonical Merkle leaf.
///
/// The tuple `(voucher, stake, token, vouch_timestamp)` uniquely identifies
/// a vouch's economically-relevant state at the time the root was computed.
pub fn hash_leaf(
    env: &Env,
    voucher: &Address,
    stake: i128,
    token: &Address,
    vouch_timestamp: u64,
) -> BytesN<32> {
    let payload = (voucher.clone(), stake, token.clone(), vouch_timestamp);
    let encoded = payload.to_xdr(env);

    let mut buf = Bytes::from_array(env, &[LEAF_PREFIX]);
    buf.append(&encoded);
    env.crypto().sha256(&buf).into()
}

/// Combine two child hashes into their parent, order-independently.
fn hash_pair(env: &Env, a: &BytesN<32>, b: &BytesN<32>) -> BytesN<32> {
    let aa = a.to_array();
    let bb = b.to_array();
    let (lo, hi) = if aa <= bb { (aa, bb) } else { (bb, aa) };

    let mut buf = [0u8; 65];
    buf[0] = NODE_PREFIX;
    buf[1..33].copy_from_slice(&lo);
    buf[33..65].copy_from_slice(&hi);

    let bytes = Bytes::from_array(env, &buf);
    env.crypto().sha256(&bytes).into()
}

/// Root committed to by an empty vouch set. Domain-separated from both leaf
/// and internal-node hashes so it can never collide with a real root.
fn empty_root(env: &Env) -> BytesN<32> {
    let bytes = Bytes::from_array(env, &[EMPTY_PREFIX]);
    env.crypto().sha256(&bytes).into()
}

/// Sort leaves by byte value, independent of the order they were passed in.
fn canonical_order(leaves: &Vec<BytesN<32>>) -> alloc::vec::Vec<[u8; 32]> {
    let mut sorted: alloc::vec::Vec<[u8; 32]> = alloc::vec::Vec::new();
    for leaf in leaves.iter() {
        sorted.push(leaf.to_array());
    }
    sorted.sort();
    sorted
}

/// Build a Merkle root from a set of leaf hashes (see [`hash_leaf`]).
///
/// Deterministic regardless of input order: leaves are canonically sorted
/// first. Duplicate leaf hashes (e.g. two structurally identical vouches)
/// are handled correctly — they simply pair with each other or with their
/// nearest neighbor like any other node, with no special-casing required,
/// since pair hashing does not assume distinctness.
pub fn build_merkle_root(env: &Env, leaves: Vec<BytesN<32>>) -> BytesN<32> {
    if leaves.is_empty() {
        return empty_root(env);
    }

    let sorted = canonical_order(&leaves);
    let mut level: alloc::vec::Vec<[u8; 32]> = sorted;

    while level.len() > 1 {
        let n = level.len();
        let mut next: alloc::vec::Vec<[u8; 32]> = alloc::vec::Vec::new();
        let mut i = 0usize;
        while i + 1 < n {
            let a = BytesN::from_array(env, &level[i]);
            let b = BytesN::from_array(env, &level[i + 1]);
            next.push(hash_pair(env, &a, &b).to_array());
            i += 2;
        }
        if i < n {
            // Odd node out: promote unchanged, never duplicate (see module docs).
            next.push(level[i]);
        }
        level = next;
    }

    BytesN::from_array(env, &level[0])
}

/// Generate a Merkle inclusion proof for `leaf` against the given leaf set.
///
/// Returns `None` if `leaf` is not present in `leaves`. This is provided for
/// tests and off-chain tooling that reconstructs the same tree; the
/// canonical, gas-cheap way to verify a proof on-chain is
/// [`verify_inclusion`], which never needs the full leaf set.
pub fn generate_proof(env: &Env, leaves: Vec<BytesN<32>>, leaf: &BytesN<32>) -> Option<Vec<BytesN<32>>> {
    if leaves.is_empty() {
        return None;
    }

    let mut level = canonical_order(&leaves);
    let target = leaf.to_array();
    let mut idx = level.iter().position(|x| *x == target)?;

    let mut proof: Vec<BytesN<32>> = Vec::new(env);

    while level.len() > 1 {
        let n = level.len();
        let mut next: alloc::vec::Vec<[u8; 32]> = alloc::vec::Vec::new();
        let mut i = 0usize;
        while i + 1 < n {
            if i == idx || i + 1 == idx {
                let sibling = if i == idx { level[i + 1] } else { level[i] };
                proof.push_back(BytesN::from_array(env, &sibling));
                idx = next.len();
            }
            let a = BytesN::from_array(env, &level[i]);
            let b = BytesN::from_array(env, &level[i + 1]);
            next.push(hash_pair(env, &a, &b).to_array());
            i += 2;
        }
        if i < n {
            if i == idx {
                idx = next.len();
            }
            next.push(level[i]);
        }
        level = next;
    }

    Some(proof)
}

/// Verify that `leaf` is included under `root`, given an inclusion `proof`
/// produced by [`generate_proof`]. Runs in `O(proof.len())` and never
/// touches the full vouch list, so a third party can verify a single vouch
/// was part of a committed set without re-deriving the whole tree.
pub fn verify_inclusion(env: &Env, root: &BytesN<32>, leaf: &BytesN<32>, proof: &Vec<BytesN<32>>) -> bool {
    let mut computed = leaf.clone();
    for sibling in proof.iter() {
        computed = hash_pair(env, &computed, &sibling);
    }
    &computed == root
}

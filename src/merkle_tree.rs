/// Stub merkle_tree module - implementation pending.
/// Provides Merkle root computation for vouch snapshots.
use soroban_sdk::{Bytes, Env, Vec};

/// Build a Merkle root from a list of leaf byte arrays.
/// Returns a 32-byte Bytes value representing the root.
pub fn build_merkle_root(_env: &Env, _leaves: Vec<Bytes>) -> Bytes {
    // Stub: returns 32 zero bytes
    Bytes::from_array(_env, &[0u8; 32])
}

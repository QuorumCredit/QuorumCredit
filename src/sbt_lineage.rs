//! # SBT Lineage Tracking (Issue #1741)
//!
//! Tracks the credential ancestry (lineage) of each SBT so that verifiers can
//! reconstruct the full chain of provenance — e.g. which original credential
//! a renewed or delegated one descends from.
//!
//! ## Lineage graph model
//!
//! Each SBT has at most one direct parent (`parent_sbt_id = 0` means root /
//! no ancestor).  The graph is therefore a *forest of chains*, not a DAG.
//! This keeps on-chain storage simple and traversal O(depth).
//!
//! ```text
//! root_sbt (id=1, parent=0)
//!   └── child_sbt (id=2, parent=1)
//!         └── grandchild_sbt (id=3, parent=2)
//! ```
//!
//! ## `trace_sbt_lineage`
//!
//! Returns the full ancestor chain as an ordered `Vec<u64>` starting with the
//! given SBT and ending at the root.  If a cycle is detected (corrupt state),
//! traversal stops and the partial chain is returned.
//!
//! ## Access control
//!
//! - `record_sbt_lineage` requires admin-threshold approval, since it writes
//!   the canonical parent relationship.
//! - `trace_sbt_lineage` is a read-only view and requires no auth.

#![allow(unused)]

use soroban_sdk::{contracttype, symbol_short, Address, Env, Vec};

use crate::errors::ContractError;
use crate::helpers::{require_admin_approval, require_not_paused};
use crate::types::{DataKey, PERSISTENT_TTL_TARGET_LEDGERS, PERSISTENT_TTL_THRESHOLD_LEDGERS};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Maximum number of hops followed during lineage traversal (cycle guard).
pub const MAX_LINEAGE_DEPTH: u32 = 100;

// ── Data Structures ───────────────────────────────────────────────────────────

/// A single node in the lineage graph.
#[contracttype]
#[derive(Clone, Debug)]
pub struct SbtLineageNode {
    /// The SBT this node describes.
    pub sbt_id: u64,
    /// Direct parent SBT id; `0` means this is a root credential.
    pub parent_sbt_id: u64,
    /// The holder address of this SBT.
    pub holder: Address,
    /// Ledger timestamp when this lineage record was created.
    pub registered_at: u64,
    /// Optional human-readable label (e.g. "renewal", "delegation").
    pub label: soroban_sdk::String,
}

// ── Storage helpers ───────────────────────────────────────────────────────────

fn load_node(env: &Env, sbt_id: u64) -> Option<SbtLineageNode> {
    env.storage()
        .persistent()
        .get(&DataKey::SbtLineage(sbt_id))
}

fn save_node(env: &Env, node: &SbtLineageNode) {
    env.storage()
        .persistent()
        .set(&DataKey::SbtLineage(node.sbt_id), node);
    env.storage().persistent().extend_ttl(
        &DataKey::SbtLineage(node.sbt_id),
        PERSISTENT_TTL_THRESHOLD_LEDGERS,
        PERSISTENT_TTL_TARGET_LEDGERS,
    );
}

// ── Public entry points ───────────────────────────────────────────────────────

/// Issue #1741 — Register the lineage relationship for an SBT.
///
/// Records that `sbt_id` descends from `parent_sbt_id` (pass `0` for a root
/// credential with no ancestor).  Requires admin approval since it canonically
/// anchors the credential ancestry chain.
///
/// # Errors
/// - [`ContractError::ContractPaused`]
/// - [`ContractError::InvalidAmount`] — `sbt_id` is 0.
/// - [`ContractError::InvalidStateTransition`] — a lineage record for
///   `sbt_id` already exists.
pub fn record_sbt_lineage(
    env: &Env,
    admin_signers: Vec<Address>,
    sbt_id: u64,
    parent_sbt_id: u64,
    holder: Address,
    label: soroban_sdk::String,
) -> Result<(), ContractError> {
    require_not_paused(env)?;
    require_admin_approval(env, &admin_signers);

    if sbt_id == 0 {
        return Err(ContractError::InvalidAmount);
    }

    if load_node(env, sbt_id).is_some() {
        return Err(ContractError::InvalidStateTransition);
    }

    // Prevent trivial self-cycles
    if parent_sbt_id == sbt_id {
        return Err(ContractError::InvalidAmount);
    }

    let node = SbtLineageNode {
        sbt_id,
        parent_sbt_id,
        holder: holder.clone(),
        registered_at: env.ledger().timestamp(),
        label: label.clone(),
    };

    save_node(env, &node);

    env.events().publish(
        (symbol_short!("sbt_lin"), symbol_short!("record")),
        (sbt_id, parent_sbt_id, holder),
    );

    Ok(())
}

/// Issue #1741 — Trace the full lineage of `sbt_id`.
///
/// Returns a `Vec<u64>` ordered from `sbt_id` (index 0) up to the root
/// ancestor (last element).  Traversal halts at [`MAX_LINEAGE_DEPTH`] to
/// guard against cycles.
///
/// An SBT with no registered lineage node returns a single-element vec
/// containing `sbt_id` itself.
pub fn trace_sbt_lineage(env: &Env, sbt_id: u64) -> Vec<u64> {
    let mut chain: Vec<u64> = Vec::new(env);
    let mut current = sbt_id;
    let mut depth: u32 = 0;

    loop {
        chain.push_back(current);
        depth += 1;

        if depth >= MAX_LINEAGE_DEPTH {
            break;
        }

        match load_node(env, current) {
            None => break,
            Some(node) => {
                let parent = node.parent_sbt_id;
                // parent == 0 means root; stop here
                if parent == 0 {
                    break;
                }
                // Cycle detection: parent already in chain
                let already_seen = chain.iter().any(|id| id == parent);
                if already_seen {
                    break;
                }
                current = parent;
            }
        }
    }

    chain
}

/// Issue #1741 — Read a single lineage node for `sbt_id`.
pub fn get_sbt_lineage_node(env: &Env, sbt_id: u64) -> Option<SbtLineageNode> {
    load_node(env, sbt_id)
}

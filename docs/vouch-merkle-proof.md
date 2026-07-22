# Vouch Merkle Proofs

**Issue**: #936

## Overview

A borrower's vouch set can be committed to a single 32-byte Merkle root, stored on-chain. A third party who already knows a specific vouch's plaintext fields — `voucher`, `stake`, `token`, `vouch_timestamp` — can then prove that vouch was part of the committed set without reading or re-deriving the borrower's full vouch list. This is useful for off-chain indexers, auditors, and cross-contract integrations that need to cite "this vouch existed at root R" cheaply and verifiably.

The implementation lives in `src/merkle_tree.rs` (tree construction and proof verification, no storage access) and `src/vouch.rs` (the contract-facing operations that read a borrower's stored vouches and wire them into the tree).

## Contract entrypoints

| Function | Description |
|---|---|
| `compute_and_store_merkle_root(borrower)` | Reads the borrower's current `Vec<VouchRecord>`, builds a Merkle root over it, and persists a `VouchMerkleRoot { root, vouch_count, computed_at }` record under `DataKey::VouchMerkleRoot(borrower)`. Fails with `NoVouchesForBorrower` if the borrower has no vouches. |
| `get_merkle_root(borrower)` | Reads the most recently stored `VouchMerkleRoot` for a borrower, if any. |
| `hash_vouch_leaf(voucher, stake, token, vouch_timestamp)` | Hashes a single vouch's plaintext fields into the canonical leaf format (see below). A prover needs this to derive the `leaf` argument to `verify_vouch_inclusion`. |
| `verify_vouch_inclusion(root, leaf, proof)` | Returns `true` iff `leaf` is included in the tree committed to by `root`, given an inclusion `proof`. Runs in `O(proof.len())` — i.e. `O(log n)` for a balanced tree — and never touches the borrower's stored vouch list. |

A root only reflects the vouch set as of the ledger when `compute_and_store_merkle_root` was last called; it is a snapshot, not a live view. Callers that need freshness should re-call it before relying on the root.

## Leaf format

Each leaf commits to one vouch's `(voucher, stake, token, vouch_timestamp)` tuple:

```
leaf = SHA256(0x00 || XDR(voucher, stake, token, vouch_timestamp))
```

`XDR(...)` is Soroban's canonical `ToXdr` serialization of the 4-tuple — the same encoding the host uses for `Val`s, so it is unambiguous across the `Address`, `i128`, and `u64` field types without any custom byte-packing logic. The `0x00` prefix is a domain-separation tag (see below).

Only these four fields are committed. Other `VouchRecord` fields (`expiry_timestamp`, `delegate`, `chain_id`) are not part of the leaf; a proof establishes that a vouch with this voucher/stake/token/timestamp existed, not the full record state.

## Internal node format and tree shape

Two child hashes `a` and `b` combine into their parent via:

```
parent = SHA256(0x01 || min(a, b) || max(a, b))
```

Sorting the pair before hashing makes proof verification **position-independent** — a proof is just an ordered list of sibling hashes with no accompanying left/right direction bits, because the verifier always recombines `(current, sibling)` the same sorted way regardless of which side `sibling` was on in the original tree.

**Domain separation.** Leaf hashes are prefixed with `0x00` and internal-node hashes with `0x01` (mirroring RFC 6962 / Certificate Transparency's approach) before hashing. This guarantees a leaf hash can never be produced by the same computation as an internal-node hash, which closes the classic "second preimage" Merkle forgery: without separation, an attacker who can influence two leaves could get their parent node's hash accepted as if it were itself a valid leaf.

**Canonical ordering.** Before the tree is built, leaves are sorted by byte value. The root therefore depends only on the *set* of vouches, never on the order `env.storage()` happens to return them in. Rebuilding the tree from the same vouch set always yields the same root, regardless of storage-iteration order.

**Odd levels.** When a level has an odd number of nodes, the leftover node is promoted **unchanged** to the next level rather than being duplicated and hashed with itself. Duplicating the last node is the mechanism behind the CVE-2012-2459-style Merkle malleability bug (padding a leaf set to forge a different root over the same effective data); promoting it instead avoids that class of bug entirely.

**Duplicate leaves.** If two vouches happen to hash to the same leaf (e.g. two structurally identical `VouchRecord`s), no special handling is needed: pair-hashing does not assume distinctness, so duplicates combine with their neighbors like any other node. A proof for a duplicated leaf proves "a vouch with these fields was included," not which particular storage slot it came from.

**Empty set.** `build_merkle_root` on zero leaves returns a sentinel `SHA256(0x02)` — domain-separated from both leaf (`0x00`) and internal-node (`0x01`) hashes, so it can never collide with a genuine root. In practice this path isn't reachable through the contract, since `compute_and_store_merkle_root` rejects empty vouch sets with `NoVouchesForBorrower` before calling into the tree builder.

## Generating a proof

Proof generation (`crate::merkle_tree::generate_proof`) needs the full leaf set and is intended to run **off-chain** — e.g. in an indexer that already tracks every `VouchRecord` for a borrower — or in tests. It is not exposed as a contract entrypoint: exposing it on-chain would require iterating the borrower's entire vouch list inside a single invocation, defeating the point of a compact proof. A proof is simply the list of sibling hashes encountered walking from a leaf to the root; anyone reproducing the leaf/node hash rules above from the same vouch data (or reading it from `src/merkle_tree.rs`) can generate one independently of the reference implementation.

## Guarantees

- **Soundness**: `verify_vouch_inclusion` returns `true` only for leaves that were genuinely part of the committed set. Forging a proof requires a SHA-256 preimage or second-preimage attack.
- **Determinism**: `compute_and_store_merkle_root` produces the same root for the same vouch set regardless of storage-iteration order.
- **Cheap verification**: `verify_vouch_inclusion` costs `O(log n)` hash operations and does not read the borrower's vouch list. Gas benchmarks (`src/merkle_tree_test.rs`, `gas_benchmarks` module) measure both operations at 5/25/50/100-vouch borrower sizes; at 100 vouches, root computation costs roughly 1.2M CPU instructions and proof verification roughly 70K — both well under 5% of Soroban mainnet's 100M-instruction per-invocation limit.

## Non-guarantees / out of scope

- **Freshness**: a root is a snapshot from whenever it was last computed. `verify_vouch_inclusion` cannot tell a caller whether the borrower's vouch set has since changed; check `VouchMerkleRoot.computed_at` and re-derive if staleness matters.
- **Exclusion proofs**: this scheme only proves inclusion. It cannot prove a given vouch is *absent* from the set.
- **Field authenticity beyond the four committed fields**: a proof establishes that `(voucher, stake, token, vouch_timestamp)` was part of the tree, not that any other `VouchRecord` field (e.g. `delegate`, `expiry_timestamp`) held a particular value.

## Tests

`src/merkle_tree_test.rs` (registered as `tests::merkle_tree_test` in `src/tests.rs`) covers:

- Root determinism regardless of leaf order, including with duplicate leaves.
- Domain separation between leaf and raw-payload hashes.
- Inclusion proofs verifying for every genuine member of a tree (including an odd-sized, 17-leaf tree, to exercise lone-node promotion).
- Inclusion proofs failing for non-members, for proofs borrowed from a different leaf, for tampered (single-byte-flipped or truncated) proofs, and against the wrong root.
- Proptest-based fuzzing (`fuzz` submodule) generating random vouch sets and confirming inclusion holds for members and fails for non-members and byte-tampered proofs across many randomized cases.
- Gas benchmarks (`gas_benchmarks` submodule) at 5/25/50/100-vouch sizes for both `build_merkle_root` and `verify_inclusion`.

## See also

- [README.md — Vouching](../README.md#how-it-works) for the general vouching mechanics this feature builds on top of.
- [`docs/vouch-cooldown-bypass-1056.md`](vouch-cooldown-bypass-1056.md) for another vouch-adjacent feature with a similar governance shape.
- `src/merkle_tree.rs` for the reference implementation.

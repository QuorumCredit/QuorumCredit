//! Cross-chain event relay (issue #969 / roadmap #86).
//!
//! Where [`crate::cross_chain`] mirrors *loans and reputation* across bridges,
//! this module provides the generic transport beneath that: an ordered,
//! replay-protected stream of events that bridge relayers carry between chains.
//!
//! Outbound: the contract records events destined for another chain in a
//! per-destination sequence. Each call assigns a monotonic sequence number and
//! publishes a Soroban event that off-chain relayers observe. An acknowledgement
//! cursor lets a relayer report how far it has delivered, so progress survives
//! restarts and delivered events can be pruned.
//!
//! Inbound: a relayer submits an event that was signed off-chain by the source
//! chain's configured relay key. The contract verifies the Ed25519 signature,
//! enforces a freshness window, and consumes both the attestation nonce and the
//! event's `(source_chain, seq)` slot, so a relayed event is accepted at most
//! once even if a relayer resubmits it.
//!
//! Signature verification, nonce replay protection, and the freshness window
//! deliberately match [`crate::cross_chain`] so operators reason about one
//! bridge-security model rather than two.

use crate::{helpers, ContractError};
use soroban_sdk::{contracttype, xdr::ToXdr, Address, Bytes, BytesN, Env, Symbol};

/// Attestation freshness window. Older relay attestations are rejected even
/// when correctly signed.
pub const MAX_RELAY_EVENT_AGE_SECS: u64 = 60 * 60;
/// Allowance for clock differences between the source and destination ledgers.
pub const MAX_RELAY_FUTURE_SKEW_SECS: u64 = 60;

/// A single cross-chain event. The same shape is used for outbound storage and
/// for inbound verification, so an event emitted on one chain deserialises
/// byte-for-byte on another.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelayEvent {
    /// Monotonic per-destination sequence assigned by the emitting chain.
    pub seq: u64,
    /// Chain the event originated on.
    pub source_chain: u32,
    /// Chain the event is addressed to.
    pub dest_chain: u32,
    /// Application-defined event kind (e.g. `loan_settled`).
    pub event_type: Symbol,
    /// Opaque application payload, interpreted by the destination handler.
    pub payload: Bytes,
    /// Ledger timestamp at which the event was emitted on the source chain.
    pub emitted_at: u64,
}

/// Off-chain signature plus the replay-protection fields it covers.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelayAttestation {
    pub signature: BytesN<64>,
    pub nonce: u64,
    pub timestamp: u64,
}

/// Exact, domain-separated value the source chain's relay key must sign.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelayInboundPayload {
    pub contract: Address,
    pub event: RelayEvent,
    pub nonce: u64,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
enum RelayKey {
    /// Ed25519 verification key for events arriving from a source chain.
    RelayBridgeKey(u32),
    /// Next outbound sequence number for a destination chain.
    OutboundSeq(u32),
    /// Stored outbound event by (destination chain, sequence).
    OutboundEvent(u32, u64),
    /// Consumed inbound attestation nonce by (source chain, nonce).
    ProcessedNonce(u32, u64),
    /// Consumed inbound event slot by (source chain, sequence).
    ProcessedSeq(u32, u64),
    /// Highest acknowledged outbound sequence for a destination chain.
    LastAck(u32),
}

// ─── Key management ──────────────────────────────────────────────────────────

/// Configure or rotate the Ed25519 key used to verify events relayed *from*
/// `source_chain`.
#[allow(deprecated)]
pub fn set_relay_key(
    env: Env,
    admin_signers: soroban_sdk::Vec<Address>,
    source_chain: u32,
    public_key: BytesN<32>,
) -> Result<(), ContractError> {
    helpers::require_admin_approval(&env, &admin_signers);
    if source_chain == 0 {
        return Err(ContractError::InvalidRelayChain);
    }
    env.storage()
        .persistent()
        .set(&RelayKey::RelayBridgeKey(source_chain), &public_key);
    env.events().publish(
        (
            soroban_sdk::symbol_short!("relay"),
            soroban_sdk::symbol_short!("keyset"),
        ),
        source_chain,
    );
    Ok(())
}

// ─── Outbound ────────────────────────────────────────────────────────────────

fn assign_seq(env: &Env, dest_chain: u32) -> u64 {
    let key = RelayKey::OutboundSeq(dest_chain);
    let current: u64 = env.storage().persistent().get(&key).unwrap_or(0);
    let next = current.saturating_add(1);
    env.storage().persistent().set(&key, &next);
    next
}

/// Record an outbound event for `dest_chain`, assign it the next sequence
/// number, persist it, and publish a Soroban event for relayers. Returns the
/// assigned sequence.
///
/// Internal: other contract modules call this when a protocol action needs to
/// propagate to another chain. The public entry point [`relay_emit`] wraps this
/// behind admin approval so it cannot be driven by arbitrary callers.
#[allow(deprecated)]
pub(crate) fn record_outbound(
    env: &Env,
    dest_chain: u32,
    event_type: Symbol,
    payload: Bytes,
) -> Result<u64, ContractError> {
    if dest_chain == 0 {
        return Err(ContractError::InvalidRelayChain);
    }

    let seq = assign_seq(env, dest_chain);
    let event = RelayEvent {
        seq,
        source_chain: 0, // 0 = this (native Stellar) chain as the source.
        dest_chain,
        event_type: event_type.clone(),
        payload,
        emitted_at: env.ledger().timestamp(),
    };

    env.storage()
        .persistent()
        .set(&RelayKey::OutboundEvent(dest_chain, seq), &event);

    env.events().publish(
        (
            soroban_sdk::symbol_short!("relay"),
            soroban_sdk::symbol_short!("out"),
        ),
        (dest_chain, seq, event_type),
    );
    Ok(seq)
}

/// Admin-gated entry point to enqueue an outbound relay event.
pub fn relay_emit(
    env: Env,
    admin_signers: soroban_sdk::Vec<Address>,
    dest_chain: u32,
    event_type: Symbol,
    payload: Bytes,
) -> Result<u64, ContractError> {
    helpers::require_admin_approval(&env, &admin_signers);
    record_outbound(&env, dest_chain, event_type, payload)
}

// ─── Inbound ─────────────────────────────────────────────────────────────────

/// Canonical bytes the source chain's relay key must sign for an inbound event.
pub fn relay_attestation_message(env: &Env, event: &RelayEvent, nonce: u64, timestamp: u64) -> Bytes {
    RelayInboundPayload {
        contract: env.current_contract_address(),
        event: event.clone(),
        nonce,
        timestamp,
    }
    .to_xdr(env)
}

fn verify_relay_attestation(
    env: &Env,
    event: &RelayEvent,
    attestation: &RelayAttestation,
) -> Result<(), ContractError> {
    if event.source_chain == 0 || event.source_chain == event.dest_chain {
        return Err(ContractError::InvalidRelayChain);
    }

    let now = env.ledger().timestamp();
    if attestation.timestamp > now.saturating_add(MAX_RELAY_FUTURE_SKEW_SECS) {
        return Err(ContractError::RelayEventFromFuture);
    }
    if now.saturating_sub(attestation.timestamp) > MAX_RELAY_EVENT_AGE_SECS {
        return Err(ContractError::RelayEventExpired);
    }

    let nonce_key = RelayKey::ProcessedNonce(event.source_chain, attestation.nonce);
    if env.storage().persistent().has(&nonce_key) {
        return Err(ContractError::RelayReplayDetected);
    }
    let seq_key = RelayKey::ProcessedSeq(event.source_chain, event.seq);
    if env.storage().persistent().has(&seq_key) {
        return Err(ContractError::RelayEventAlreadyProcessed);
    }

    let public_key: BytesN<32> = env
        .storage()
        .persistent()
        .get(&RelayKey::RelayBridgeKey(event.source_chain))
        .ok_or(ContractError::RelayKeyNotConfigured)?;
    let message = relay_attestation_message(env, event, attestation.nonce, attestation.timestamp);

    // Soroban aborts the invocation on an invalid Ed25519 signature. Because
    // the nonce/seq writes happen afterwards, a bad signature cannot consume a
    // nonce or mark a sequence processed.
    env.crypto()
        .ed25519_verify(&public_key, &message, &attestation.signature);
    Ok(())
}

/// Verify a relayed event and atomically consume its nonce and `(source, seq)`
/// slot. Idempotent under resubmission: a duplicate seq or nonce is rejected
/// rather than reprocessed.
#[allow(deprecated)]
pub fn relay_message(
    env: Env,
    event: RelayEvent,
    attestation: RelayAttestation,
) -> Result<(), ContractError> {
    verify_relay_attestation(&env, &event, &attestation)?;

    env.storage().persistent().set(
        &RelayKey::ProcessedNonce(event.source_chain, attestation.nonce),
        &true,
    );
    env.storage()
        .persistent()
        .set(&RelayKey::ProcessedSeq(event.source_chain, event.seq), &true);

    env.events().publish(
        (
            soroban_sdk::symbol_short!("relay"),
            soroban_sdk::symbol_short!("in"),
        ),
        (event.source_chain, event.seq, event.event_type),
    );
    Ok(())
}

// ─── Acknowledgement cursor ──────────────────────────────────────────────────

/// Record that a relayer has delivered every outbound event up to and including
/// `up_to_seq` for `dest_chain`. Monotonic: an acknowledgement may not move the
/// cursor backwards.
pub fn acknowledge_relay(
    env: Env,
    admin_signers: soroban_sdk::Vec<Address>,
    dest_chain: u32,
    up_to_seq: u64,
) -> Result<(), ContractError> {
    helpers::require_admin_approval(&env, &admin_signers);
    ack_core(&env, dest_chain, up_to_seq)
}

#[allow(deprecated)]
fn ack_core(env: &Env, dest_chain: u32, up_to_seq: u64) -> Result<(), ContractError> {
    if dest_chain == 0 {
        return Err(ContractError::InvalidRelayChain);
    }
    let key = RelayKey::LastAck(dest_chain);
    let current: u64 = env.storage().persistent().get(&key).unwrap_or(0);
    if up_to_seq < current {
        return Err(ContractError::RelayAckRegression);
    }
    env.storage().persistent().set(&key, &up_to_seq);
    env.events().publish(
        (
            soroban_sdk::symbol_short!("relay"),
            soroban_sdk::symbol_short!("ack"),
        ),
        (dest_chain, up_to_seq),
    );
    Ok(())
}

// ─── Queries ─────────────────────────────────────────────────────────────────

pub fn get_outbound_event(env: Env, dest_chain: u32, seq: u64) -> Option<RelayEvent> {
    env.storage()
        .persistent()
        .get(&RelayKey::OutboundEvent(dest_chain, seq))
}

pub fn latest_outbound_seq(env: Env, dest_chain: u32) -> u64 {
    env.storage()
        .persistent()
        .get(&RelayKey::OutboundSeq(dest_chain))
        .unwrap_or(0)
}

pub fn last_acknowledged_seq(env: Env, dest_chain: u32) -> u64 {
    env.storage()
        .persistent()
        .get(&RelayKey::LastAck(dest_chain))
        .unwrap_or(0)
}

pub fn is_relay_processed(env: Env, source_chain: u32, seq: u64) -> bool {
    env.storage()
        .persistent()
        .has(&RelayKey::ProcessedSeq(source_chain, seq))
}

pub fn is_relay_nonce_used(env: Env, source_chain: u32, nonce: u64) -> bool {
    env.storage()
        .persistent()
        .has(&RelayKey::ProcessedNonce(source_chain, nonce))
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use soroban_sdk::{
        contract,
        testutils::Ledger,
        symbol_short, Address, Bytes, BytesN, Env,
    };

    #[contract]
    struct StorageContract;

    struct Fixture {
        env: Env,
        contract: Address,
        signer: SigningKey,
    }

    impl Fixture {
        fn new() -> Self {
            let env = Env::default();
            env.ledger().set_timestamp(10_000);
            let contract = env.register(StorageContract, ());
            let signer = SigningKey::from_bytes(&[9; 32]);
            let public_key = BytesN::from_array(&env, signer.verifying_key().as_bytes());
            env.as_contract(&contract, || {
                env.storage()
                    .persistent()
                    .set(&RelayKey::RelayBridgeKey(1), &public_key);
            });
            Self {
                env,
                contract,
                signer,
            }
        }

        fn event(&self, seq: u64) -> RelayEvent {
            RelayEvent {
                seq,
                source_chain: 1,
                dest_chain: 2,
                event_type: symbol_short!("settled"),
                payload: Bytes::from_array(&self.env, &[1, 2, 3, 4]),
                emitted_at: 9_000,
            }
        }

        fn sign(&self, event: &RelayEvent, nonce: u64, timestamp: u64) -> RelayAttestation {
            let message = self.env.as_contract(&self.contract, || {
                relay_attestation_message(&self.env, event, nonce, timestamp)
            });
            let signature = self.signer.sign(&message.to_alloc_vec()).to_bytes();
            RelayAttestation {
                signature: BytesN::from_array(&self.env, &signature),
                nonce,
                timestamp,
            }
        }

        fn receive(
            &self,
            event: RelayEvent,
            attestation: RelayAttestation,
        ) -> Result<(), ContractError> {
            self.env.as_contract(&self.contract, || {
                relay_message(self.env.clone(), event, attestation)
            })
        }
    }

    #[test]
    fn valid_inbound_event_is_processed() {
        let f = Fixture::new();
        let event = f.event(1);
        let att = f.sign(&event, 1, 10_000);
        assert_eq!(f.receive(event, att), Ok(()));
        assert!(f
            .env
            .as_contract(&f.contract, || is_relay_processed(f.env.clone(), 1, 1)));
        assert!(f
            .env
            .as_contract(&f.contract, || is_relay_nonce_used(f.env.clone(), 1, 1)));
    }

    #[test]
    #[should_panic]
    fn tampered_event_fails_signature() {
        let f = Fixture::new();
        let event = f.event(1);
        let att = f.sign(&event, 1, 10_000);
        let mut tampered = event;
        tampered.payload = Bytes::from_array(&f.env, &[9, 9, 9, 9]);
        let _ = f.receive(tampered, att);
    }

    #[test]
    fn replayed_nonce_is_rejected() {
        let f = Fixture::new();
        let first = f.event(1);
        assert_eq!(f.receive(first.clone(), f.sign(&first, 7, 10_000)), Ok(()));
        // Different seq, same nonce → replay.
        let second = f.event(2);
        assert_eq!(
            f.receive(second, f.sign(&f.event(2), 7, 10_001)),
            Err(ContractError::RelayReplayDetected)
        );
    }

    #[test]
    fn duplicate_seq_is_rejected() {
        let f = Fixture::new();
        let event = f.event(5);
        assert_eq!(f.receive(event.clone(), f.sign(&event, 1, 10_000)), Ok(()));
        // Same seq, fresh nonce → already processed.
        assert_eq!(
            f.receive(event.clone(), f.sign(&event, 2, 10_001)),
            Err(ContractError::RelayEventAlreadyProcessed)
        );
    }

    #[test]
    fn expired_attestation_is_rejected() {
        let f = Fixture::new();
        let event = f.event(1);
        let att = f.sign(&event, 1, 10_000 - MAX_RELAY_EVENT_AGE_SECS - 1);
        assert_eq!(f.receive(event, att), Err(ContractError::RelayEventExpired));
    }

    #[test]
    fn future_attestation_is_rejected() {
        let f = Fixture::new();
        let event = f.event(1);
        let att = f.sign(&event, 1, 10_000 + MAX_RELAY_FUTURE_SKEW_SECS + 1);
        assert_eq!(
            f.receive(event, att),
            Err(ContractError::RelayEventFromFuture)
        );
    }

    #[test]
    fn unconfigured_source_chain_is_rejected() {
        let f = Fixture::new();
        let mut event = f.event(1);
        event.source_chain = 8; // no key configured for chain 8
        let att = f.sign(&event, 1, 10_000);
        assert_eq!(
            f.receive(event, att),
            Err(ContractError::RelayKeyNotConfigured)
        );
    }

    #[test]
    fn same_source_and_dest_is_rejected() {
        let f = Fixture::new();
        let mut event = f.event(1);
        event.dest_chain = 1;
        let att = f.sign(&event, 1, 10_000);
        assert_eq!(f.receive(event, att), Err(ContractError::InvalidRelayChain));
    }

    #[test]
    fn outbound_seq_is_monotonic_per_destination() {
        let f = Fixture::new();
        f.env.as_contract(&f.contract, || {
            assert_eq!(
                record_outbound(&f.env, 2, symbol_short!("a"), Bytes::new(&f.env)),
                Ok(1)
            );
            assert_eq!(
                record_outbound(&f.env, 2, symbol_short!("b"), Bytes::new(&f.env)),
                Ok(2)
            );
            // Independent counter for a different destination.
            assert_eq!(
                record_outbound(&f.env, 3, symbol_short!("c"), Bytes::new(&f.env)),
                Ok(1)
            );
            assert_eq!(latest_outbound_seq(f.env.clone(), 2), 2);
            assert_eq!(latest_outbound_seq(f.env.clone(), 3), 1);
        });
    }

    #[test]
    fn outbound_event_is_retrievable() {
        let f = Fixture::new();
        f.env.as_contract(&f.contract, || {
            let payload = Bytes::from_array(&f.env, &[7, 7]);
            let seq =
                record_outbound(&f.env, 2, symbol_short!("loan"), payload.clone()).unwrap();
            let stored = get_outbound_event(f.env.clone(), 2, seq).unwrap();
            assert_eq!(stored.dest_chain, 2);
            assert_eq!(stored.source_chain, 0);
            assert_eq!(stored.payload, payload);
            assert_eq!(stored.event_type, symbol_short!("loan"));
        });
    }

    #[test]
    fn zero_destination_is_rejected() {
        let f = Fixture::new();
        f.env.as_contract(&f.contract, || {
            assert_eq!(
                record_outbound(&f.env, 0, symbol_short!("x"), Bytes::new(&f.env)),
                Err(ContractError::InvalidRelayChain)
            );
        });
    }

    #[test]
    fn acknowledge_is_monotonic() {
        let f = Fixture::new();
        f.env.as_contract(&f.contract, || {
            assert_eq!(ack_core(&f.env, 2, 5), Ok(()));
            assert_eq!(last_acknowledged_seq(f.env.clone(), 2), 5);
            // Moving forward is fine.
            assert_eq!(ack_core(&f.env, 2, 9), Ok(()));
            // Moving backward is rejected.
            assert_eq!(ack_core(&f.env, 2, 4), Err(ContractError::RelayAckRegression));
        });
    }

    #[test]
    fn nonce_and_seq_scope_is_per_source_chain() {
        let f = Fixture::new();
        // Configure a second source chain.
        let public_key = BytesN::from_array(&f.env, f.signer.verifying_key().as_bytes());
        f.env.as_contract(&f.contract, || {
            f.env
                .storage()
                .persistent()
                .set(&RelayKey::RelayBridgeKey(3), &public_key);
        });
        let first = f.event(1);
        assert_eq!(f.receive(first.clone(), f.sign(&first, 1, 10_000)), Ok(()));

        // Same seq + nonce but different source chain → accepted.
        let mut second = f.event(1);
        second.source_chain = 3;
        assert_eq!(f.receive(second.clone(), f.sign(&second, 1, 10_001)), Ok(()));
    }
}

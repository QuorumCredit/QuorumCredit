#[cfg(test)]
mod relay_tests {
    use crate::types::{DataKey, RelayAttestation, RelayEvent};
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, Bytes, BytesN, Env, Symbol, Vec,
    };

    struct Setup {
        env: Env,
        client: QuorumCreditContractClient<'static>,
        contract_id: Address,
        admins: Vec<Address>,
    }

    fn setup(admin_threshold: u32, num_admins: usize) -> Setup {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let mut admins = Vec::new(&env);
        for _ in 0..num_admins {
            admins.push_back(Address::generate(&env));
        }

        let token_id = env.register_stellar_asset_contract_v2(admins.get(0).unwrap().clone());
        let contract_id = env.register_contract(None, QuorumCreditContract);

        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &1_000_000_000);

        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &admin_threshold, &token_id.address());

        env.ledger().with_mut(|l| l.timestamp = 120);

        Setup {
            env,
            client,
            contract_id,
            admins,
        }
    }

    #[test]
    fn test_set_relay_key_stores_public_key() {
        let s = setup(1, 1);
        let source_chain: u32 = 1;
        let public_key: BytesN<32> = BytesN::from_array(&s.env, &[42u8; 32]);

        s.client.set_relay_key(&s.admins, &source_chain, &public_key);

        let stored_key = s.env.as_contract(&s.contract_id, || {
            s.env
                .storage()
                .persistent()
                .get::<DataKey, BytesN<32>>(&DataKey::RelayPublicKey(source_chain))
                .unwrap()
        });

        assert_eq!(stored_key, public_key);
    }

    #[test]
    fn test_set_relay_key_rejects_invalid_chain() {
        let s = setup(1, 1);
        let public_key: BytesN<32> = BytesN::from_array(&s.env, &[42u8; 32]);

        let result = s.client.try_set_relay_key(&s.admins, &0, &public_key);
        assert!(result.is_err());
    }

    #[test]
    fn test_relay_emit_creates_event() {
        let s = setup(1, 1);
        let dest_chain: u32 = 2;
        let event_type = Symbol::new(&s.env, "test_event");
        let payload = Bytes::from_slice(&s.env, &[1, 2, 3, 4, 5]);

        let seq = s.client.relay_emit(&s.admins, &dest_chain, &event_type, &payload);

        assert_eq!(seq, 1);

        let stored_seq = s.env.as_contract(&s.contract_id, || {
            s.env
                .storage()
                .persistent()
                .get::<DataKey, u64>(&DataKey::OutboundRelaySeq(dest_chain))
                .unwrap()
        });

        assert_eq!(stored_seq, 1);
    }

    #[test]
    fn test_relay_emit_increments_sequence() {
        let s = setup(1, 1);
        let dest_chain: u32 = 2;
        let event_type = Symbol::new(&s.env, "test_event");
        let payload = Bytes::from_slice(&s.env, &[1, 2, 3, 4, 5]);

        let seq1 = s.client.relay_emit(&s.admins, &dest_chain, &event_type, &payload);
        let seq2 = s.client.relay_emit(&s.admins, &dest_chain, &event_type, &payload);

        assert_eq!(seq1, 1);
        assert_eq!(seq2, 2);
    }

    #[test]
    fn test_get_outbound_relay_event_returns_stored_event() {
        let s = setup(1, 1);
        let dest_chain: u32 = 2;
        let event_type = Symbol::new(&s.env, "test_event");
        let payload = Bytes::from_slice(&s.env, &[1, 2, 3, 4, 5]);

        let seq = s.client.relay_emit(&s.admins, &dest_chain, &event_type, &payload);

        let retrieved = s.client.get_outbound_relay_event(&dest_chain, &seq).unwrap();

        assert_eq!(retrieved.dest_chain, dest_chain);
        assert_eq!(retrieved.seq, seq);
    }

    #[test]
    fn test_latest_outbound_relay_seq_returns_correct_seq() {
        let s = setup(1, 1);
        let dest_chain: u32 = 2;
        let event_type = Symbol::new(&s.env, "test_event");
        let payload = Bytes::from_slice(&s.env, &[1, 2, 3, 4, 5]);

        s.client.relay_emit(&s.admins, &dest_chain, &event_type, &payload);
        s.client.relay_emit(&s.admins, &dest_chain, &event_type, &payload);
        s.client.relay_emit(&s.admins, &dest_chain, &event_type, &payload);

        let latest = s.client.latest_outbound_relay_seq(&dest_chain);

        assert_eq!(latest, 3);
    }

    #[test]
    fn test_relay_message_rejects_unregistered_key() {
        let s = setup(1, 1);
        let source_chain: u32 = 1;
        let event = RelayEvent {
            source_chain,
            dest_chain: 2,
            event_type: Symbol::new(&s.env, "test"),
            payload: Bytes::from_slice(&s.env, &[1, 2, 3]),
            seq: 1,
        };

        let attestation = RelayAttestation {
            signature: BytesN::from_array(&s.env, &[42u8; 64]),
            nonce: 1,
            timestamp: 120,
        };

        let result = s.client.try_relay_message(&event, &attestation);
        assert!(result.is_err());
    }

    #[test]
    fn test_relay_message_rejects_replay_nonce() {
        let s = setup(1, 1);
        let source_chain: u32 = 1;
        let nonce: u64 = 42;
        let timestamp = s.env.ledger().timestamp();

        let public_key: BytesN<32> = BytesN::from_array(&s.env, &[1u8; 32]);
        s.client.set_relay_key(&s.admins, &source_chain, &public_key);

        let event = RelayEvent {
            source_chain,
            dest_chain: 2,
            event_type: Symbol::new(&s.env, "test"),
            payload: Bytes::from_slice(&s.env, &[1, 2, 3]),
            seq: 1,
        };

        let attestation = RelayAttestation {
            signature: BytesN::from_array(&s.env, &[42u8; 64]),
            nonce,
            timestamp,
        };

        s.env.as_contract(&s.contract_id, || {
            s.env
                .storage()
                .persistent()
                .set(&DataKey::RelayNonceUsed(source_chain, nonce), &true);
        });

        let result = s.client.try_relay_message(&event, &attestation);
        assert!(result.is_err());
    }

    #[test]
    fn test_is_relay_processed_tracks_processed_events() {
        let s = setup(1, 1);
        let source_chain: u32 = 1;
        let seq: u64 = 42;

        let is_processed_before = s.client.is_relay_processed(&source_chain, &seq);
        assert!(!is_processed_before);

        s.env.as_contract(&s.contract_id, || {
            s.env
                .storage()
                .persistent()
                .set(&DataKey::RelayEventProcessed(source_chain, seq), &true);
        });

        let is_processed_after = s.client.is_relay_processed(&source_chain, &seq);
        assert!(is_processed_after);
    }

    #[test]
    fn test_is_relay_nonce_used_tracks_used_nonces() {
        let s = setup(1, 1);
        let source_chain: u32 = 1;
        let nonce: u64 = 42;

        let is_used_before = s.client.is_relay_nonce_used(&source_chain, &nonce);
        assert!(!is_used_before);

        s.env.as_contract(&s.contract_id, || {
            s.env
                .storage()
                .persistent()
                .set(&DataKey::RelayNonceUsed(source_chain, nonce), &true);
        });

        let is_used_after = s.client.is_relay_nonce_used(&source_chain, &nonce);
        assert!(is_used_after);
    }

    #[test]
    fn test_acknowledge_relay_updates_last_acked_seq() {
        let s = setup(1, 1);
        let dest_chain: u32 = 2;
        let seq: u64 = 42;

        s.client.acknowledge_relay(&s.admins, &dest_chain, &seq);

        let last_acked = s.client.last_acknowledged_relay_seq(&dest_chain);
        assert_eq!(last_acked, seq);
    }

    #[test]
    fn test_acknowledge_relay_rejects_regression() {
        let s = setup(1, 1);
        let dest_chain: u32 = 2;

        s.client.acknowledge_relay(&s.admins, &dest_chain, &100);

        let result = s.client.try_acknowledge_relay(&s.admins, &dest_chain, &50);
        assert!(result.is_err());
    }

    #[test]
    fn test_relay_event_stores_all_fields() {
        let s = setup(1, 1);
        let dest_chain: u32 = 2;
        let source_chain: u32 = 1;
        let event_type = Symbol::new(&s.env, "transfer");
        let payload = Bytes::from_slice(&s.env, &[10, 20, 30, 40, 50]);

        let seq = s.client.relay_emit(&s.admins, &dest_chain, &event_type, &payload);

        let stored = s.client.get_outbound_relay_event(&dest_chain, &seq).unwrap();

        assert_eq!(stored.dest_chain, dest_chain);
        assert_eq!(stored.seq, seq);
        assert_eq!(stored.event_type, event_type);
        assert_eq!(stored.payload, payload);
    }
}

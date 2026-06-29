#![cfg(test)]

mod sep10_test_util;

mod batch_attestation_tests {
    use soroban_sdk::{
        testutils::{Address as _, Ledger, LedgerInfo},
        Address, Bytes, Env, Vec,
    };
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    use anchorkit::contract::{
        AnchorKitContract, AnchorKitContractClient, AttestationInput, MAX_BATCH_SIZE,
        BATCH_ATTESTATION_RATE_MULTIPLIER,
    };
    use crate::sep10_test_util::{register_attestor_with_sep10, sign_payload};

    fn make_env() -> Env {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set(LedgerInfo {
            timestamp: 1_000_000,
            protocol_version: 21,
            sequence_number: 100,
            network_id: Default::default(),
            base_reserve: 0,
            min_persistent_entry_ttl: 4096,
            min_temp_entry_ttl: 16,
            max_entry_ttl: 6312000,
        });
        env
    }

    fn setup(env: &Env) -> (AnchorKitContractClient, Address, SigningKey) {
        let contract_id = env.register_contract(None, AnchorKitContract);
        let client = AnchorKitContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        client.initialize(&admin);

        // Configure rate limit high enough for batch tests
        use anchorkit::RateLimitConfig;
        client.set_rate_limit_config(&RateLimitConfig {
            max_submissions: 1000,
            window_length: 10000,
        });

        let sk = SigningKey::generate(&mut OsRng);
        let attestor = Address::generate(env);
        register_attestor_with_sep10(env, &client, &attestor, &attestor, &sk);
        (client, attestor, sk)
    }

    fn payload(env: &Env, seed: u8, salt: u8) -> Bytes {
        let mut b = Bytes::new(env);
        for i in 0..32u8 {
            b.push_back(seed.wrapping_add(i).wrapping_add(salt));
        }
        b
    }

    fn make_input(
        env: &Env,
        attestor: &Address,
        sk: &SigningKey,
        subject: &Address,
        seed: u8,
        salt: u8,
    ) -> AttestationInput {
        let ph = payload(env, seed, salt);
        let sig = sign_payload(env, sk, &ph);
        AttestationInput {
            issuer: attestor.clone(),
            subject: subject.clone(),
            timestamp: 1_000_001u64,
            payload_hash: ph,
            signature: sig,
        }
    }

    // ── Successful batch of 5 ────────────────────────────────────────────────

    #[test]
    fn batch_of_5_succeeds_and_returns_sequential_ids() {
        let env = make_env();
        let (client, attestor, sk) = setup(&env);
        let subject = Address::generate(&env);

        let mut inputs: Vec<AttestationInput> = Vec::new(&env);
        for i in 0u8..5 {
            inputs.push_back(make_input(&env, &attestor, &sk, &subject, 0xA0, i));
        }

        let ids = client.submit_attestation_batch(&attestor, &inputs);
        assert_eq!(ids.len(), 5);
        // IDs should be sequential starting from 0
        for (idx, id) in ids.iter().enumerate() {
            assert_eq!(id, idx as u64);
        }
        // Each attestation should be retrievable
        for id in ids.iter() {
            let att = client.get_attestation(&id);
            assert_eq!(att.issuer, attestor);
        }
    }

    // ── Empty batch returns empty Vec ────────────────────────────────────────

    #[test]
    fn empty_batch_returns_empty_vec() {
        let env = make_env();
        let (client, attestor, _sk) = setup(&env);

        let inputs: Vec<AttestationInput> = Vec::new(&env);
        let ids = client.submit_attestation_batch(&attestor, &inputs);
        assert_eq!(ids.len(), 0);
    }

    // ── Batch with one duplicate is fully rejected ───────────────────────────

    #[test]
    fn batch_with_duplicate_is_fully_rejected() {
        let env = make_env();
        let (client, attestor, sk) = setup(&env);
        let subject = Address::generate(&env);

        // Submit first attestation individually
        let ph = payload(&env, 0xBB, 0);
        let sig = sign_payload(&env, &sk, &ph);
        client.submit_attestation(&attestor, &subject, &1_000_001u64, &ph, &sig);

        // Build a batch where one entry is a duplicate
        let mut inputs: Vec<AttestationInput> = Vec::new(&env);
        inputs.push_back(make_input(&env, &attestor, &sk, &subject, 0xCC, 1));
        inputs.push_back(make_input(&env, &attestor, &sk, &subject, 0xCC, 2));
        // Duplicate: same payload_hash as the individually submitted one
        inputs.push_back(AttestationInput {
            issuer: attestor.clone(),
            subject: subject.clone(),
            timestamp: 1_000_001u64,
            payload_hash: ph,
            signature: sig,
        });

        let result = client.try_submit_attestation_batch(&attestor, &inputs);
        assert!(result.is_err(), "batch with duplicate should be rejected");

        // State unchanged: no new IDs beyond the original 1
        let next_id = client.get_attestation(&0u64);
        assert_eq!(next_id.id, 0);
        // The IDs 1 and 2 should not exist
        assert!(client.try_get_attestation(&1u64).is_err());
    }

    // ── Batch with invalid signature is fully rejected ───────────────────────

    #[test]
    fn batch_with_invalid_signature_is_fully_rejected() {
        let env = make_env();
        let (client, attestor, sk) = setup(&env);
        let subject = Address::generate(&env);

        let wrong_sk = SigningKey::generate(&mut OsRng);

        let mut inputs: Vec<AttestationInput> = Vec::new(&env);
        inputs.push_back(make_input(&env, &attestor, &sk, &subject, 0xD0, 0));
        inputs.push_back(make_input(&env, &attestor, &sk, &subject, 0xD0, 1));
        // Bad signature entry: sign with wrong key
        let ph_bad = payload(&env, 0xD0, 2);
        let bad_sig = sign_payload(&env, &wrong_sk, &ph_bad);
        inputs.push_back(AttestationInput {
            issuer: attestor.clone(),
            subject: subject.clone(),
            timestamp: 1_000_001u64,
            payload_hash: ph_bad,
            signature: bad_sig,
        });

        let result = client.try_submit_attestation_batch(&attestor, &inputs);
        assert!(result.is_err(), "batch with invalid signature should be rejected");

        // No attestations written — ID 0 should not exist
        assert!(client.try_get_attestation(&0u64).is_err());
    }

    // ── Batch exceeding MAX_BATCH_SIZE is rejected ───────────────────────────

    #[test]
    fn batch_exceeding_max_size_is_rejected() {
        let env = make_env();
        let (client, attestor, sk) = setup(&env);
        let subject = Address::generate(&env);

        let mut inputs: Vec<AttestationInput> = Vec::new(&env);
        for i in 0u8..(MAX_BATCH_SIZE as u8 + 1) {
            inputs.push_back(make_input(&env, &attestor, &sk, &subject, 0xE0, i));
        }

        let result = client.try_submit_attestation_batch(&attestor, &inputs);
        assert!(result.is_err(), "batch over MAX_BATCH_SIZE should be rejected");
    }

    // ── Rate limit consumed proportionally ──────────────────────────────────

    #[test]
    fn rate_limit_consumed_proportionally() {
        let env = make_env();
        let contract_id = env.register_contract(None, AnchorKitContract);
        let client = AnchorKitContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);

        use anchorkit::RateLimitConfig;
        // Allow exactly BATCH_ATTESTATION_RATE_MULTIPLIER * 3 slots (enough for batch of 3)
        let max = BATCH_ATTESTATION_RATE_MULTIPLIER * 3;
        client.set_rate_limit_config(&RateLimitConfig {
            max_submissions: max,
            window_length: 10000,
        });

        let sk = SigningKey::generate(&mut OsRng);
        let attestor = Address::generate(&env);
        register_attestor_with_sep10(&env, &client, &attestor, &attestor, &sk);
        let subject = Address::generate(&env);

        // Batch of 3 should succeed (uses exactly max slots)
        let mut inputs: Vec<AttestationInput> = Vec::new(&env);
        for i in 0u8..3 {
            inputs.push_back(make_input(&env, &attestor, &sk, &subject, 0xF0, i));
        }
        let ids = client.submit_attestation_batch(&attestor, &inputs);
        assert_eq!(ids.len(), 3);

        // A batch of 1 more should fail — rate limit exhausted
        let mut more: Vec<AttestationInput> = Vec::new(&env);
        more.push_back(make_input(&env, &attestor, &sk, &subject, 0xF1, 0));
        let result = client.try_submit_attestation_batch(&attestor, &more);
        assert!(result.is_err(), "rate limit should be exhausted after first batch");
    }
}

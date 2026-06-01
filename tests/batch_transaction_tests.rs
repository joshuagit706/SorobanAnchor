//! Tests for batch transaction queries and summary functions.
//!
//! Acceptance criteria verified:
//! 1. `get_transactions_in_range` returns records within the specified ID range.
//! 2. `get_transactions_in_range` respects the batch cap (max 100).
//! 3. `get_transactions_in_range` returns an empty vec for an inverted range.
//! 4. `summarize_transactions_by_status` returns correct per-state counts.
//! 5. `summarize_transactions_by_status` returns all-zero summary when no transactions exist.
//! 6. Tracker-level `get_transactions_in_range` and `summarize_transactions_by_status` work in dev mode.

#![cfg(test)]

mod batch_transaction_tests {
    use soroban_sdk::{
        testutils::{Address as _, Ledger, LedgerInfo},
        Address, Env, String,
    };

    use anchorkit::transaction_state_tracker::{
        TransactionState, TransactionStateTracker, TransactionSummary,
    };
    use anchorkit::contract::{AnchorKitContract, AnchorKitContractClient};

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn make_env() -> Env {
        let env = Env::default();
        env.mock_all_auths();
        env
    }

    fn set_ledger(env: &Env, ts: u64) {
        env.ledger().set(LedgerInfo {
            timestamp: ts,
            protocol_version: 21,
            sequence_number: 0,
            network_id: Default::default(),
            base_reserve: 0,
            min_persistent_entry_ttl: 4096,
            min_temp_entry_ttl: 16,
            max_entry_ttl: 6_312_000,
        });
    }

    fn deploy(env: &Env) -> (AnchorKitContractClient, Address) {
        let contract_id = env.register_contract(None, AnchorKitContract);
        let client = AnchorKitContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        client.initialize(&admin);
        (client, admin)
    }

    // -----------------------------------------------------------------------
    // TransactionStateTracker (dev-mode) — batch range query
    // -----------------------------------------------------------------------

    #[test]
    fn tracker_get_transactions_in_range_returns_matching_records() {
        let env = make_env();
        set_ledger(&env, 1000);
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = Address::generate(&env);

        for i in 1u64..=5 {
            tracker.create_transaction(i, initiator.clone(), &env).unwrap();
        }

        let batch = tracker.get_transactions_in_range(2, 4, 10, &env).unwrap();
        assert_eq!(batch.len(), 3);
        assert_eq!(batch[0].transaction_id, 2);
        assert_eq!(batch[1].transaction_id, 3);
        assert_eq!(batch[2].transaction_id, 4);
    }

    #[test]
    fn tracker_get_transactions_in_range_full_range() {
        let env = make_env();
        set_ledger(&env, 1000);
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = Address::generate(&env);

        for i in 1u64..=5 {
            tracker.create_transaction(i, initiator.clone(), &env).unwrap();
        }

        let batch = tracker.get_transactions_in_range(1, 5, 100, &env).unwrap();
        assert_eq!(batch.len(), 5);
    }

    #[test]
    fn tracker_get_transactions_in_range_inverted_returns_empty() {
        let env = make_env();
        set_ledger(&env, 1000);
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = Address::generate(&env);

        tracker.create_transaction(1, initiator.clone(), &env).unwrap();

        let batch = tracker.get_transactions_in_range(5, 1, 10, &env).unwrap();
        assert!(batch.is_empty());
    }

    #[test]
    fn tracker_get_transactions_in_range_respects_limit() {
        let env = make_env();
        set_ledger(&env, 1000);
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = Address::generate(&env);

        for i in 1u64..=10 {
            tracker.create_transaction(i, initiator.clone(), &env).unwrap();
        }

        // Request limit of 3 from a range of 10
        let batch = tracker.get_transactions_in_range(1, 10, 3, &env).unwrap();
        assert_eq!(batch.len(), 3);
    }

    #[test]
    fn tracker_get_transactions_in_range_cap_at_100() {
        let env = make_env();
        set_ledger(&env, 1000);
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = Address::generate(&env);

        for i in 1u64..=110 {
            tracker.create_transaction(i, initiator.clone(), &env).unwrap();
        }

        // Even if limit > 100, result is capped at 100
        let batch = tracker.get_transactions_in_range(1, 110, 200, &env).unwrap();
        assert_eq!(batch.len(), 100);
    }

    #[test]
    fn tracker_get_transactions_in_range_single_id() {
        let env = make_env();
        set_ledger(&env, 1000);
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = Address::generate(&env);

        for i in 1u64..=5 {
            tracker.create_transaction(i, initiator.clone(), &env).unwrap();
        }

        let batch = tracker.get_transactions_in_range(3, 3, 10, &env).unwrap();
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].transaction_id, 3);
    }

    // -----------------------------------------------------------------------
    // TransactionStateTracker (dev-mode) — summary by status
    // -----------------------------------------------------------------------

    #[test]
    fn tracker_summarize_empty_returns_all_zeros() {
        let env = make_env();
        set_ledger(&env, 1000);
        let tracker = TransactionStateTracker::new(true);

        let summary = tracker.summarize_transactions_by_status(&env).unwrap();
        assert_eq!(summary, TransactionSummary::default());
        assert_eq!(summary.total_count, 0);
    }

    #[test]
    fn tracker_summarize_counts_each_state_correctly() {
        let env = make_env();
        set_ledger(&env, 1000);
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = Address::generate(&env);

        // Create 5 transactions
        for i in 1u64..=5 {
            tracker.create_transaction(i, initiator.clone(), &env).unwrap();
        }
        // Move 2 to InProgress
        tracker.start_transaction(1, &env).unwrap();
        tracker.start_transaction(2, &env).unwrap();
        // Complete 1
        tracker.complete_transaction(1, &env).unwrap();
        // Fail 1
        tracker
            .fail_transaction(2, String::from_str(&env, "timeout"), &env)
            .unwrap();

        let summary = tracker.summarize_transactions_by_status(&env).unwrap();
        assert_eq!(summary.pending_count, 3);
        assert_eq!(summary.in_progress_count, 0);
        assert_eq!(summary.completed_count, 1);
        assert_eq!(summary.failed_count, 1);
        assert_eq!(summary.total_count, 5);
    }

    #[test]
    fn tracker_summarize_all_pending() {
        let env = make_env();
        set_ledger(&env, 1000);
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = Address::generate(&env);

        for i in 1u64..=4 {
            tracker.create_transaction(i, initiator.clone(), &env).unwrap();
        }

        let summary = tracker.summarize_transactions_by_status(&env).unwrap();
        assert_eq!(summary.pending_count, 4);
        assert_eq!(summary.in_progress_count, 0);
        assert_eq!(summary.completed_count, 0);
        assert_eq!(summary.failed_count, 0);
        assert_eq!(summary.total_count, 4);
    }

    #[test]
    fn tracker_summarize_all_completed() {
        let env = make_env();
        set_ledger(&env, 1000);
        let mut tracker = TransactionStateTracker::new(true);
        let initiator = Address::generate(&env);

        for i in 1u64..=3 {
            tracker.create_transaction(i, initiator.clone(), &env).unwrap();
            tracker.start_transaction(i, &env).unwrap();
            tracker.complete_transaction(i, &env).unwrap();
        }

        let summary = tracker.summarize_transactions_by_status(&env).unwrap();
        assert_eq!(summary.completed_count, 3);
        assert_eq!(summary.total_count, 3);
        assert_eq!(summary.pending_count, 0);
        assert_eq!(summary.failed_count, 0);
    }

    // -----------------------------------------------------------------------
    // Contract-level batch query (on-chain storage path)
    // -----------------------------------------------------------------------

    #[test]
    fn contract_get_transactions_in_range_returns_stored_records() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, _admin) = deploy(&env);

        let initiator = Address::generate(&env);
        for i in 1u64..=5 {
            client.create_transaction_record(&i, &initiator);
        }

        let batch = client.get_transactions_in_range(&2, &4, &10);
        assert_eq!(batch.len(), 3);
        assert_eq!(batch.get(0).unwrap().transaction_id, 2);
        assert_eq!(batch.get(1).unwrap().transaction_id, 3);
        assert_eq!(batch.get(2).unwrap().transaction_id, 4);
    }

    #[test]
    fn contract_get_transactions_in_range_inverted_returns_empty() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, _admin) = deploy(&env);

        let initiator = Address::generate(&env);
        client.create_transaction_record(&1, &initiator);

        let batch = client.get_transactions_in_range(&5, &1, &10);
        assert_eq!(batch.len(), 0);
    }

    #[test]
    fn contract_summarize_transactions_by_status_correct_counts() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, _admin) = deploy(&env);

        let initiator = Address::generate(&env);
        for i in 1u64..=4 {
            client.create_transaction_record(&i, &initiator);
        }
        // Advance tx 1: Pending -> InProgress -> Completed
        client.start_transaction_record(&1);
        client.complete_transaction_record(&1);
        // Advance tx 2: Pending -> InProgress -> Failed
        client.start_transaction_record(&2);
        client.fail_transaction_record(&2, &String::from_str(&env, "error"));

        let summary = client.summarize_transactions_by_status();
        assert_eq!(summary.pending_count, 2);
        assert_eq!(summary.in_progress_count, 0);
        assert_eq!(summary.completed_count, 1);
        assert_eq!(summary.failed_count, 1);
        assert_eq!(summary.total_count, 4);
    }

    #[test]
    fn contract_summarize_empty_returns_zeros() {
        let env = make_env();
        set_ledger(&env, 1000);
        let (client, _admin) = deploy(&env);

        let summary = client.summarize_transactions_by_status();
        assert_eq!(summary.total_count, 0);
        assert_eq!(summary.pending_count, 0);
        assert_eq!(summary.completed_count, 0);
        assert_eq!(summary.failed_count, 0);
    }
}

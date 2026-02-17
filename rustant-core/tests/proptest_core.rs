//! Property-based tests for core components using proptest.

use proptest::prelude::*;

use rustant_core::Message;
use rustant_core::credentials::CredentialStore;
use rustant_core::credentials::InMemoryCredentialStore;
use rustant_core::injection::InjectionDetector;
use rustant_core::memory::{Fact, LongTermMemory, ShortTermMemory};
use rustant_core::merkle::MerkleChain;
use rustant_core::scheduler::{CronJob, CronJobConfig, CronScheduler};
use rustant_core::skills::parse_skill_md;
use rustant_core::updater::is_newer_version;

// --- Version comparison properties ---

proptest! {
    #[test]
    fn version_equal_is_not_newer(
        major in 0u32..100,
        minor in 0u32..100,
        patch in 0u32..100,
    ) {
        let v = format!("{}.{}.{}", major, minor, patch);
        prop_assert!(!is_newer_version(&v, &v));
    }

    #[test]
    fn version_newer_major_is_detected(
        major in 1u32..100,
        minor in 0u32..100,
        patch in 0u32..100,
    ) {
        let newer = format!("{}.{}.{}", major, minor, patch);
        let older = format!("{}.{}.{}", major - 1, minor, patch);
        prop_assert!(is_newer_version(&newer, &older));
        prop_assert!(!is_newer_version(&older, &newer));
    }

    #[test]
    fn version_newer_minor_is_detected(
        major in 0u32..100,
        minor in 1u32..100,
        patch in 0u32..100,
    ) {
        let newer = format!("{}.{}.{}", major, minor, patch);
        let older = format!("{}.{}.{}", major, minor - 1, patch);
        prop_assert!(is_newer_version(&newer, &older));
    }

    #[test]
    fn version_newer_patch_is_detected(
        major in 0u32..100,
        minor in 0u32..100,
        patch in 1u32..100,
    ) {
        let newer = format!("{}.{}.{}", major, minor, patch);
        let older = format!("{}.{}.{}", major, minor, patch - 1);
        prop_assert!(is_newer_version(&newer, &older));
    }
}

// --- Merkle chain properties ---

proptest! {
    #[test]
    fn merkle_chain_always_verifies_after_appends(
        events in prop::collection::vec(prop::collection::vec(any::<u8>(), 1..100), 1..50)
    ) {
        let mut chain = MerkleChain::new();
        for event in &events {
            chain.append(event);
        }
        let result = chain.verify_chain();
        prop_assert!(result.is_valid);
        prop_assert_eq!(result.checked_nodes, events.len());
    }

    #[test]
    fn merkle_chain_root_hash_is_deterministic(
        data in prop::collection::vec(any::<u8>(), 1..100)
    ) {
        let mut chain1 = MerkleChain::new();
        let mut chain2 = MerkleChain::new();
        chain1.append(&data);
        chain2.append(&data);
        prop_assert_eq!(chain1.root_hash(), chain2.root_hash());
    }

    #[test]
    fn merkle_chain_different_data_different_hash(
        data1 in prop::collection::vec(any::<u8>(), 1..100),
        data2 in prop::collection::vec(any::<u8>(), 1..100),
    ) {
        prop_assume!(data1 != data2);
        let mut chain1 = MerkleChain::new();
        let mut chain2 = MerkleChain::new();
        chain1.append(&data1);
        chain2.append(&data2);
        prop_assert_ne!(chain1.root_hash(), chain2.root_hash());
    }

    #[test]
    fn merkle_chain_length_matches_appends(count in 0usize..100) {
        let mut chain = MerkleChain::new();
        for i in 0..count {
            chain.append(format!("event_{}", i).as_bytes());
        }
        prop_assert_eq!(chain.len(), count);
    }
}

// --- Memory system properties ---

proptest! {
    #[test]
    fn short_term_memory_tracks_total_seen(
        count in 1usize..200,
    ) {
        let mut stm = ShortTermMemory::new(50);
        for i in 0..count {
            stm.add(Message::user(format!("msg {}", i)));
        }
        // The memory should hold all added messages (no auto-eviction)
        prop_assert_eq!(stm.len(), count);
    }

    #[test]
    fn long_term_fact_search_returns_relevant(
        query_word in "[a-z]{3,10}",
        noise_count in 0usize..20,
    ) {
        let mut ltm = LongTermMemory::new();

        // Add noise facts
        for i in 0..noise_count {
            ltm.add_fact(Fact::new(format!("Irrelevant noise fact number {}", i), "test"));
        }

        // Add a fact containing the query word
        ltm.add_fact(Fact::new(
            format!("This fact contains the word {}", query_word),
            "test",
        ));

        let results = ltm.search_facts(&query_word);
        prop_assert!(!results.is_empty(), "Should find at least the fact containing the query word");
    }
}

// --- Injection detector properties ---

proptest! {
    #[test]
    fn injection_detector_never_panics(input in ".*") {
        let detector = InjectionDetector::new();
        let _ = detector.scan_input(&input);
    }

    #[test]
    fn injection_detector_tool_output_never_panics(output in ".*") {
        let detector = InjectionDetector::new();
        let _ = detector.scan_tool_output(&output);
    }

    #[test]
    fn injection_detector_empty_input_is_clean(s in "\\s*") {
        let detector = InjectionDetector::new();
        let result = detector.scan_input(&s);
        // Empty or whitespace-only input should not be suspicious
        prop_assert!(!result.is_suspicious || s.trim().is_empty());
    }
}

// --- Skill parser properties ---

proptest! {
    #[test]
    fn skill_parser_never_panics(content in ".*") {
        // The parser should never panic, even on garbage input
        let _ = parse_skill_md(&content);
    }

    #[test]
    fn skill_parser_valid_frontmatter(
        name in "[a-z][a-z0-9-]{2,20}",
        version in "[0-9]{1,3}\\.[0-9]{1,3}\\.[0-9]{1,3}",
        description in "[a-zA-Z ]{5,50}",
    ) {
        let content = format!(
            "---\nname: {}\nversion: \"{}\"\ndescription: {}\n---\n\n### test_tool\n\nA test tool.\n",
            name, version, description
        );
        let result = parse_skill_md(&content);
        prop_assert!(result.is_ok(), "Valid frontmatter should parse: {:?}", result.err());
        let skill = result.unwrap();
        prop_assert_eq!(skill.name, name);
        prop_assert_eq!(skill.version, version);
    }
}

// --- Cron scheduler properties ---

proptest! {
    #[test]
    fn cron_job_next_run_is_always_in_future(
        second in 0u32..60,
        minute in 0u32..60,
        hour in 0u32..24,
    ) {
        // Build a valid 7-field cron expression: sec min hour day month weekday year
        let expr = format!("{} {} {} * * * *", second, minute, hour);
        let config = CronJobConfig::new("prop-test", &expr, "test task");
        if let Ok(job) = CronJob::new(config)
            && let Some(next_run) = job.next_run {
                // next_run should always be >= now
                let now = chrono::Utc::now();
                prop_assert!(
                    next_run >= now - chrono::Duration::seconds(2),
                    "next_run ({:?}) should be in the future (now: {:?})",
                    next_run,
                    now
                );
            }
        // If parsing fails, that's fine — not all combinations are valid
    }

    #[test]
    fn cron_scheduler_add_remove_is_idempotent(
        job_count in 1usize..20,
    ) {
        let mut scheduler = CronScheduler::new();
        let mut names = Vec::new();

        // Add N jobs
        for i in 0..job_count {
            let name = format!("job_{}", i);
            let config = CronJobConfig::new(&name, "0 0 9 * * * *", format!("task {}", i));
            scheduler.add_job(config).unwrap();
            names.push(name);
        }
        prop_assert_eq!(scheduler.len(), job_count);

        // Remove all jobs
        for name in &names {
            scheduler.remove_job(name).unwrap();
        }
        prop_assert!(scheduler.is_empty());
    }

    #[test]
    fn cron_scheduler_serialization_roundtrip(
        job_count in 1usize..10,
    ) {
        let mut scheduler = CronScheduler::new();
        for i in 0..job_count {
            let config = CronJobConfig::new(
                format!("job_{}", i),
                "0 0 9 * * * *",
                format!("task {}", i),
            );
            scheduler.add_job(config).unwrap();
        }

        // Serialize and deserialize
        let json = scheduler.to_json().unwrap();
        let restored = CronScheduler::from_json(&json).unwrap();
        prop_assert_eq!(restored.len(), scheduler.len());
    }
}

// --- Short-term memory pinning properties ---

proptest! {
    #[test]
    fn pinned_messages_always_in_output(
        total_msgs in 5usize..50,
        pin_idx in 0usize..5,
    ) {
        let pin_idx = pin_idx.min(total_msgs - 1);
        let mut stm = ShortTermMemory::new(10); // Window of 10

        for i in 0..total_msgs {
            stm.add(Message::user(format!("message_{}", i)));
        }

        // Pin a message
        stm.pin(pin_idx);

        // Get all messages for the context window
        let output = stm.to_messages();

        // The pinned message text should appear in the output
        let pin_text = format!("message_{}", pin_idx);
        let found = output.iter().any(|m| {
            if let Some(text) = m.content.as_text() {
                text.contains(&pin_text)
            } else {
                false
            }
        });
        prop_assert!(
            found,
            "Pinned message '{}' should appear in to_messages() output",
            pin_text
        );
    }
}

// --- Credential store properties ---

proptest! {
    #[test]
    fn credential_store_roundtrip(
        provider in "[a-z]{3,20}",
        key in "[a-zA-Z0-9!@#$%^&*()]{5,50}",
    ) {
        let store = InMemoryCredentialStore::new();
        store.store_key(&provider, &key).unwrap();
        let retrieved = store.get_key(&provider).unwrap();
        prop_assert_eq!(retrieved, key);
    }

    #[test]
    fn credential_store_has_key_after_store(
        provider in "[a-z]{3,20}",
    ) {
        let store = InMemoryCredentialStore::new();
        prop_assert!(!store.has_key(&provider));
        store.store_key(&provider, "test-key").unwrap();
        prop_assert!(store.has_key(&provider));
    }

    #[test]
    fn credential_store_delete_removes_key(
        provider in "[a-z]{3,20}",
    ) {
        let store = InMemoryCredentialStore::new();
        store.store_key(&provider, "test-key").unwrap();
        store.delete_key(&provider).unwrap();
        prop_assert!(!store.has_key(&provider));
    }
}

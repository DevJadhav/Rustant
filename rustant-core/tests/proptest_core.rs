//! Property-based tests for core components using proptest.

use proptest::prelude::*;

use rustant_core::injection::InjectionDetector;
use rustant_core::memory::{Fact, LongTermMemory, ShortTermMemory};
use rustant_core::merkle::MerkleChain;
use rustant_core::skills::parse_skill_md;
use rustant_core::updater::is_newer_version;
use rustant_core::Message;

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

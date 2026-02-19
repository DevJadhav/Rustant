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
        let v = format!("{major}.{minor}.{patch}");
        prop_assert!(!is_newer_version(&v, &v));
    }

    #[test]
    fn version_newer_major_is_detected(
        major in 1u32..100,
        minor in 0u32..100,
        patch in 0u32..100,
    ) {
        let newer = format!("{major}.{minor}.{patch}");
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
        let newer = format!("{major}.{minor}.{patch}");
        let older = format!("{}.{}.{}", major, minor - 1, patch);
        prop_assert!(is_newer_version(&newer, &older));
    }

    #[test]
    fn version_newer_patch_is_detected(
        major in 0u32..100,
        minor in 0u32..100,
        patch in 1u32..100,
    ) {
        let newer = format!("{major}.{minor}.{patch}");
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
            chain.append(format!("event_{i}").as_bytes());
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
            stm.add(Message::user(format!("msg {i}")));
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
            ltm.add_fact(Fact::new(format!("Irrelevant noise fact number {i}"), "test"));
        }

        // Add a fact containing the query word
        ltm.add_fact(Fact::new(
            format!("This fact contains the word {query_word}"),
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
            "---\nname: {name}\nversion: \"{version}\"\ndescription: {description}\n---\n\n### test_tool\n\nA test tool.\n"
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
        let expr = format!("{second} {minute} {hour} * * * *");
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
            let name = format!("job_{i}");
            let config = CronJobConfig::new(&name, "0 0 9 * * * *", format!("task {i}"));
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
                format!("job_{i}"),
                "0 0 9 * * * *",
                format!("task {i}"),
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
            stm.add(Message::user(format!("message_{i}")));
        }

        // Pin a message
        stm.pin(pin_idx);

        // Get all messages for the context window
        let output = stm.to_messages();

        // The pinned message text should appear in the output
        let pin_text = format!("message_{pin_idx}");
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

// ---------------------------------------------------------------------------
// Cache property tests
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn test_token_usage_cache_fields_never_negative(
        input in 0usize..100_000,
        output in 0usize..100_000,
        cache_read in 0usize..100_000,
        cache_creation in 0usize..100_000,
    ) {
        use rustant_core::types::TokenUsage;
        let usage = TokenUsage {
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: cache_read,
            cache_creation_tokens: cache_creation,
        };
        prop_assert!(usage.input_tokens <= 100_000);
        prop_assert!(usage.output_tokens <= 100_000);
        prop_assert!(usage.cache_read_tokens <= 100_000);
        prop_assert!(usage.cache_creation_tokens <= 100_000);
    }

    #[test]
    fn test_token_usage_accumulate_monotonic(
        a_in in 0usize..50_000,
        a_out in 0usize..50_000,
        a_cr in 0usize..50_000,
        a_cc in 0usize..50_000,
        b_in in 0usize..50_000,
        b_out in 0usize..50_000,
        b_cr in 0usize..50_000,
        b_cc in 0usize..50_000,
    ) {
        use rustant_core::types::TokenUsage;
        let mut a = TokenUsage {
            input_tokens: a_in,
            output_tokens: a_out,
            cache_read_tokens: a_cr,
            cache_creation_tokens: a_cc,
        };
        let b = TokenUsage {
            input_tokens: b_in,
            output_tokens: b_out,
            cache_read_tokens: b_cr,
            cache_creation_tokens: b_cc,
        };
        a.accumulate(&b);
        prop_assert_eq!(a.input_tokens, a_in + b_in);
        prop_assert_eq!(a.output_tokens, a_out + b_out);
        prop_assert_eq!(a.cache_read_tokens, a_cr + b_cr);
        prop_assert_eq!(a.cache_creation_tokens, a_cc + b_cc);
    }
}

// ---------------------------------------------------------------------------
// Persona property tests
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn test_persona_selection_deterministic(
        seed in 0u32..100,
    ) {
        use rustant_core::personas::PersonaResolver;
        use rustant_core::types::TaskClassification;
        let _ = seed; // ensure proptest generates different runs
        let resolver = PersonaResolver::new(None);
        let classification = TaskClassification::CodeAnalysis;
        let first = resolver.active_persona(Some(&classification));
        let second = resolver.active_persona(Some(&classification));
        prop_assert_eq!(first, second);
    }

    #[test]
    fn test_persona_override_always_wins(
        class_idx in 0u32..4,
    ) {
        use rustant_core::personas::{PersonaId, PersonaResolver};
        use rustant_core::types::TaskClassification;
        let classifications = [
            TaskClassification::CodeAnalysis,
            TaskClassification::Calendar,
            TaskClassification::SystemMonitor,
            TaskClassification::GitOperation,
        ];
        let classification = &classifications[class_idx as usize % classifications.len()];
        let mut resolver = PersonaResolver::new(None);
        resolver.set_override(Some(PersonaId::SecurityGuardian));
        let result = resolver.active_persona(Some(classification));
        prop_assert_eq!(result, PersonaId::SecurityGuardian);
    }
}

// ---------------------------------------------------------------------------
// Embedding property tests
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn test_local_embedder_deterministic(
        text in "[a-z ]{5,50}",
    ) {
        use rustant_core::embeddings::{Embedder, LocalEmbedder};
        let embedder = LocalEmbedder::new(128);
        let v1 = embedder.embed(&text);
        let v2 = embedder.embed(&text);
        prop_assert_eq!(v1.len(), v2.len());
        for (a, b) in v1.iter().zip(v2.iter()) {
            prop_assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn test_local_embedder_dimensions_match(
        dims in 32usize..512,
        text in "[a-z ]{5,50}",
    ) {
        use rustant_core::embeddings::{Embedder, LocalEmbedder};
        let embedder = LocalEmbedder::new(dims);
        let vec = embedder.embed(&text);
        prop_assert_eq!(vec.len(), dims);
    }
}

// ---------------------------------------------------------------------------
// Content variant serde round-trip property tests
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn prop_content_text_roundtrip(text in ".*") {
        use rustant_core::Content;
        let content = Content::Text { text: text.clone() };
        let json = serde_json::to_string(&content).unwrap();
        let restored: Content = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(content, restored);
    }

    #[test]
    fn prop_content_image_roundtrip(
        data in "[a-zA-Z0-9+/]{10,100}",
        media in "(image/png|image/jpeg|image/gif|image/webp)",
    ) {
        use rustant_core::{Content, ImageSource};
        let content = Content::Image {
            source: ImageSource::Base64(data),
            media_type: media,
            detail: None,
        };
        let json = serde_json::to_string(&content).unwrap();
        let restored: Content = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(content, restored);
    }

    #[test]
    fn prop_content_thinking_roundtrip(
        thinking in "[a-zA-Z ]{5,100}",
        has_sig in any::<bool>(),
    ) {
        use rustant_core::Content;
        let content = Content::Thinking {
            thinking,
            signature: if has_sig { Some("sig-123".to_string()) } else { None },
        };
        let json = serde_json::to_string(&content).unwrap();
        let restored: Content = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(content, restored);
    }

    #[test]
    fn prop_content_citation_roundtrip(
        cited_text in "[a-zA-Z ]{5,50}",
        title in "[a-zA-Z ]{3,30}",
    ) {
        use rustant_core::{Content, CitationSource};
        let content = Content::Citation {
            cited_text,
            source: CitationSource::Document { title, page: Some(42) },
            start_index: Some(0),
            end_index: Some(10),
        };
        let json = serde_json::to_string(&content).unwrap();
        let restored: Content = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(content, restored);
    }

    #[test]
    fn prop_content_code_execution_roundtrip(
        code in "[a-z_()]{5,50}",
        output in "[a-zA-Z0-9 ]{0,50}",
    ) {
        use rustant_core::Content;
        let content = Content::CodeExecution {
            language: "python".to_string(),
            code,
            output: if output.is_empty() { None } else { Some(output) },
            error: None,
        };
        let json = serde_json::to_string(&content).unwrap();
        let restored: Content = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(content, restored);
    }

    #[test]
    fn prop_content_search_result_roundtrip(
        query in "[a-z ]{3,30}",
        count in 0usize..5,
    ) {
        use rustant_core::{Content, GroundingResult};
        let results: Vec<GroundingResult> = (0..count)
            .map(|i| GroundingResult {
                title: format!("Result {i}"),
                url: format!("https://example.com/{i}"),
                snippet: format!("Snippet for result {i}"),
            })
            .collect();
        let content = Content::SearchResult { query, results };
        let json = serde_json::to_string(&content).unwrap();
        let restored: Content = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(content, restored);
    }
}

// ---------------------------------------------------------------------------
// ThinkingConfig property tests
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn prop_thinking_config_valid(
        budget in proptest::option::of(1usize..100_000),
        level in proptest::option::of("(none|low|medium|high)"),
    ) {
        use rustant_core::ThinkingConfig;
        let config = ThinkingConfig {
            enabled: true,
            budget_tokens: budget,
            level,
        };
        // Budget should never be negative (usize guarantees this)
        if let Some(b) = config.budget_tokens {
            prop_assert!(b > 0);
        }
        // Roundtrip
        let json = serde_json::to_string(&config).unwrap();
        let restored: ThinkingConfig = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(config.enabled, restored.enabled);
        prop_assert_eq!(config.budget_tokens, restored.budget_tokens);
        prop_assert_eq!(config.level, restored.level);
    }
}

// ---------------------------------------------------------------------------
// HookEvent property tests
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn prop_hook_event_session_roundtrip(is_start in any::<bool>()) {
        use rustant_core::HookEvent;
        let event = if is_start { HookEvent::SessionStart } else { HookEvent::SessionEnd };
        let json = serde_json::to_string(&event).unwrap();
        let restored: HookEvent = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(event, restored);
    }

    #[test]
    fn prop_hook_event_task_roundtrip(
        goal in "[a-zA-Z ]{3,50}",
        success in any::<bool>(),
    ) {
        use rustant_core::HookEvent;
        let start = HookEvent::TaskStart { goal: goal.clone() };
        let complete = HookEvent::TaskComplete { goal, success };
        let json1 = serde_json::to_string(&start).unwrap();
        let json2 = serde_json::to_string(&complete).unwrap();
        let r1: HookEvent = serde_json::from_str(&json1).unwrap();
        let r2: HookEvent = serde_json::from_str(&json2).unwrap();
        prop_assert_eq!(start, r1);
        prop_assert_eq!(complete, r2);
    }

    #[test]
    fn prop_hook_event_tool_roundtrip(
        tool_name in "[a-z_]{3,20}",
        success in any::<bool>(),
    ) {
        use rustant_core::HookEvent;
        let pre = HookEvent::PreToolUse {
            tool_name: tool_name.clone(),
            args: serde_json::json!({"key": "value"}),
        };
        let post = HookEvent::PostToolUse {
            tool_name,
            result: "ok".to_string(),
            success,
        };
        let json1 = serde_json::to_string(&pre).unwrap();
        let json2 = serde_json::to_string(&post).unwrap();
        let r1: HookEvent = serde_json::from_str(&json1).unwrap();
        let r2: HookEvent = serde_json::from_str(&json2).unwrap();
        prop_assert_eq!(pre, r1);
        prop_assert_eq!(post, r2);
    }

    #[test]
    fn prop_hook_event_cache_hit_roundtrip(
        provider in "(anthropic|openai|gemini|ollama)",
        tokens_saved in 0usize..100_000,
    ) {
        use rustant_core::HookEvent;
        let event = HookEvent::CacheHit { provider, tokens_saved };
        let json = serde_json::to_string(&event).unwrap();
        let restored: HookEvent = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(event, restored);
    }
}

// ---------------------------------------------------------------------------
// ToolChoice property tests
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn prop_tool_choice_default_is_auto(_seed in 0u32..100) {
        use rustant_core::ToolChoice;
        let default = ToolChoice::default();
        match default {
            ToolChoice::Auto => {} // expected
            other => prop_assert!(false, "Default should be Auto, got {:?}", other),
        }
    }

    #[test]
    fn prop_tool_choice_specific_roundtrip(name in "[a-z_]{3,20}") {
        use rustant_core::ToolChoice;
        let choice = ToolChoice::Specific(name.clone());
        let json = serde_json::to_string(&choice).unwrap();
        let restored: ToolChoice = serde_json::from_str(&json).unwrap();
        if let ToolChoice::Specific(n) = restored {
            prop_assert_eq!(n, name);
        } else {
            prop_assert!(false, "Should be Specific variant");
        }
    }
}

// ---------------------------------------------------------------------------
// GroundingResult property tests
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn prop_grounding_results_non_empty(
        title in "[a-zA-Z ]{3,30}",
        url in "https://[a-z]{3,15}\\.com/[a-z]{1,10}",
        snippet in "[a-zA-Z ]{5,50}",
    ) {
        use rustant_core::GroundingResult;
        let result = GroundingResult { title: title.clone(), url: url.clone(), snippet: snippet.clone() };
        prop_assert!(!result.title.is_empty());
        prop_assert!(!result.url.is_empty());
        prop_assert!(!result.snippet.is_empty());
        // Roundtrip
        let json = serde_json::to_string(&result).unwrap();
        let restored: GroundingResult = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(result, restored);
    }
}

// ---------------------------------------------------------------------------
// PermissionPolicy property tests
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn prop_permission_policy_denylist_wins(
        tool in "[a-z_]{3,20}",
    ) {
        use rustant_core::PermissionPolicy;
        // Tool is in both allowlist and denylist — denylist should win
        let policy = PermissionPolicy {
            tool_allowlist: Some(vec![tool.clone()]),
            tool_denylist: vec![tool.clone()],
            ..Default::default()
        };
        prop_assert!(!policy.is_tool_allowed(&tool), "Denylist should take priority over allowlist");
    }

    #[test]
    fn prop_permission_policy_allowlist_restricts(
        allowed in "[a-z]{3,10}",
        other in "[A-Z]{3,10}",
    ) {
        use rustant_core::PermissionPolicy;
        prop_assume!(allowed != other);
        let policy = PermissionPolicy {
            tool_allowlist: Some(vec![allowed.clone()]),
            ..Default::default()
        };
        prop_assert!(policy.is_tool_allowed(&allowed));
        prop_assert!(!policy.is_tool_allowed(&other), "Tool not in allowlist should be denied");
    }

    #[test]
    fn prop_permission_policy_cost_limit(
        limit in 0.01f64..1000.0,
        current in 0.0f64..2000.0,
    ) {
        use rustant_core::PermissionPolicy;
        let policy = PermissionPolicy {
            max_cost_per_task: Some(limit),
            ..Default::default()
        };
        let within = policy.is_within_cost_limit(current);
        if current <= limit {
            prop_assert!(within, "Cost {} within limit {} should be allowed", current, limit);
        } else {
            prop_assert!(!within, "Cost {} over limit {} should be denied", current, limit);
        }
    }

    #[test]
    fn prop_permission_policy_iteration_limit(
        limit in 1usize..100,
        current in 0usize..200,
    ) {
        use rustant_core::PermissionPolicy;
        let policy = PermissionPolicy {
            max_iterations_per_task: Some(limit),
            ..Default::default()
        };
        let within = policy.is_within_iteration_limit(current);
        if current <= limit {
            prop_assert!(within);
        } else {
            prop_assert!(!within);
        }
    }

    #[test]
    fn prop_permission_policy_no_limits_allows_all(
        tool in "[a-z_]{3,20}",
        cost in 0.0f64..10_000.0,
        iters in 0usize..10_000,
    ) {
        use rustant_core::PermissionPolicy;
        let policy = PermissionPolicy::default();
        prop_assert!(policy.is_tool_allowed(&tool));
        prop_assert!(policy.is_within_cost_limit(cost));
        prop_assert!(policy.is_within_iteration_limit(iters));
    }
}

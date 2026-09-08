use chilli_model::capabilities::{CapabilityTier, ModelCapabilityProfile, ObservationSample};

#[test]
fn test_cold_start_defaults_to_unknown_tier() {
    let profile = ModelCapabilityProfile::new("ollama", "qwen3:1.7b", 8192);
    let assessment = profile.assess();
    assert_eq!(assessment.current_tier, CapabilityTier::Unknown);
    assert!(assessment.is_cold_start);
}

#[test]
fn test_hysteresis_requires_sustained_evidence_for_tier_transition() {
    let mut profile = ModelCapabilityProfile::new("ollama", "qwen3:1.7b", 8192);

    // 1 observation high score should NOT immediately promote past cold start until evidence_window met
    profile.record_observation(ObservationSample {
        tool_call_accuracy: 1.0,
        instruction_following: 1.0,
        verification_success: 1.0,
        recovery_success: 1.0,
    });
    let initial_assessment = profile.assess();
    assert_eq!(initial_assessment.current_tier, CapabilityTier::Unknown);

    // Record enough consecutive high observations (evidence_window = 3)
    for _ in 0..3 {
        profile.record_observation(ObservationSample {
            tool_call_accuracy: 1.0,
            instruction_following: 1.0,
            verification_success: 1.0,
            recovery_success: 1.0,
        });
    }
    let promoted_assessment = profile.assess();
    assert_ne!(promoted_assessment.current_tier, CapabilityTier::Unknown);
    assert!(!promoted_assessment.is_cold_start);

    // Single low score should NOT immediately demote due to hysteresis demotion threshold & window
    profile.record_observation(ObservationSample {
        tool_call_accuracy: 0.0,
        instruction_following: 0.0,
        verification_success: 0.0,
        recovery_success: 0.0,
    });
    let after_single_fail = profile.assess();
    assert_eq!(
        after_single_fail.current_tier,
        promoted_assessment.current_tier
    );
}

#[test]
fn test_hysteresis_no_soft_lock_on_fluctuating_observations() {
    let mut profile = ModelCapabilityProfile::new("groq", "llama-3.3-70b", 8192);

    // Initial transition out of Unknown to Frontier
    for _ in 0..3 {
        profile.record_observation(ObservationSample {
            tool_call_accuracy: 1.0,
            instruction_following: 1.0,
            verification_success: 1.0,
            recovery_success: 1.0,
        });
    }
    assert_eq!(profile.assess().current_tier, CapabilityTier::Frontier);

    // Sustained lower performance (Medium range)
    for _ in 0..5 {
        profile.record_observation(ObservationSample {
            tool_call_accuracy: 0.7,
            instruction_following: 0.7,
            verification_success: 0.7,
            recovery_success: 0.7,
        });
    }

    // Demotion should succeed and NOT soft-lock at Frontier
    let demoted_assessment = profile.assess();
    assert_eq!(demoted_assessment.current_tier, CapabilityTier::Medium);
}

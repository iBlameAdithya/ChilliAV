use chilli_model::capabilities::ReasoningTier;
use chilli_model::router::{CapabilityRegistry, Requirement};

#[test]
fn test_default_capability_profiles_fallback() {
    let registry = CapabilityRegistry::with_default_profiles();
    let reqs = vec![Requirement::MinReasoningTier(ReasoningTier::High)];
    let selected = registry.select(&reqs).unwrap();
    assert!(
        selected.model_id.contains("sonnet")
            || selected.model_id.contains("gpt-4o")
            || selected.model_id.contains("gemini")
    );
}

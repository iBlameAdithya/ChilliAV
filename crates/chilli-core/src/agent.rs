//! Core agent execution state machine module.

use crate::verifier::{VerificationExecutor, VerificationGate, VerificationResult};
use chilli_model::capabilities::{ModelCapabilityProfile, ObservationSample};
use chilli_orchestrator::complexity::TaskComplexity;
use chilli_orchestrator::policy::{ExecutionPolicyEngine, TaskExecutionState};
use std::path::Path;

pub struct AgentEngine;

pub struct CapabilityAgentRunner {
    gate: VerificationGate,
    pub execution_state: TaskExecutionState,
}

impl CapabilityAgentRunner {
    pub fn new(executor: Box<dyn VerificationExecutor>) -> Self {
        Self {
            gate: VerificationGate::new(vec![executor]),
            execution_state: TaskExecutionState::default(),
        }
    }

    pub fn with_gate(gate: VerificationGate) -> Self {
        Self {
            gate,
            execution_state: TaskExecutionState::default(),
        }
    }

    pub async fn execute_step(
        &mut self,
        workspace_root: &Path,
        profile: &mut ModelCapabilityProfile,
        complexity: &TaskComplexity,
    ) -> VerificationResult {
        let assessment = profile.assess();
        let _strategy =
            ExecutionPolicyEngine::select_strategy(&assessment, complexity, &self.execution_state);

        self.execution_state.attempt_count += 1;

        let res = self.gate.run_verification(workspace_root).await;

        match res {
            Ok(VerificationResult::Passed) => {
                profile.record_observation(ObservationSample {
                    tool_call_accuracy: 1.0,
                    instruction_following: 1.0,
                    verification_success: 1.0,
                    recovery_success: 1.0,
                });
                self.execution_state.verification_failures = 0;
                self.execution_state.repeated_failure_count = 0;
                VerificationResult::Passed
            }
            Ok(VerificationResult::Failed(slice)) => {
                profile.record_observation(ObservationSample {
                    tool_call_accuracy: 0.5,
                    instruction_following: 0.5,
                    verification_success: 0.0,
                    recovery_success: 0.0,
                });
                self.execution_state.verification_failures += 1;
                self.execution_state.repeated_failure_count += 1;
                VerificationResult::Failed(slice)
            }
            Err(e) => {
                profile.record_observation(ObservationSample {
                    tool_call_accuracy: 0.0,
                    instruction_following: 0.0,
                    verification_success: 0.0,
                    recovery_success: 0.0,
                });
                self.execution_state.verification_failures += 1;
                self.execution_state.repeated_failure_count += 1;
                VerificationResult::Failed(crate::verifier::DiagnosticSlice {
                    category: crate::verifier::FailureCategory::Unknown,
                    primary_message: format!("Infrastructure error during verification: {}", e),
                    file: None,
                    line: None,
                    evidence: vec![e.to_string()],
                    context_budget_bytes: 1024,
                })
            }
        }
    }
}

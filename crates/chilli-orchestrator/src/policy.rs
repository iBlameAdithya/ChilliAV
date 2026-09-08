use crate::complexity::TaskComplexity;
use chilli_model::capabilities::{CapabilityAssessment, CapabilityTier};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskExecutionState {
    pub attempt_count: usize,
    pub verification_failures: usize,
    pub recent_tool_failures: usize,
    pub repeated_failure_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlastBudget {
    pub max_files: usize,
    pub max_lines: usize,
    pub max_symbols: usize,
    pub max_destructive_operations: usize,
    pub requires_checkpoint: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutionStrategy {
    MicroStep {
        blast_budget: BlastBudget,
        mandatory_verification_per_mutation: bool,
        single_hypothesis_prompting: bool,
    },
    Guided {
        blast_budget: BlastBudget,
        checkpoint_verification: bool,
    },
    MacroAutonomous {
        blast_budget: BlastBudget,
        phase_level_verification: bool,
    },
}

pub struct ExecutionPolicyEngine;

impl ExecutionPolicyEngine {
    pub fn select_strategy(
        assessment: &CapabilityAssessment,
        complexity: &TaskComplexity,
        state: &TaskExecutionState,
    ) -> ExecutionStrategy {
        let is_high_risk = complexity.is_high_risk()
            || state.repeated_failure_count > 1
            || state.verification_failures > 1;

        match assessment.current_tier {
            CapabilityTier::Unknown | CapabilityTier::Micro => ExecutionStrategy::MicroStep {
                blast_budget: BlastBudget {
                    max_files: 1,
                    max_lines: 50,
                    max_symbols: 3,
                    max_destructive_operations: 0,
                    requires_checkpoint: true,
                },
                mandatory_verification_per_mutation: true,
                single_hypothesis_prompting: true,
            },
            CapabilityTier::Small => {
                if is_high_risk {
                    ExecutionStrategy::MicroStep {
                        blast_budget: BlastBudget {
                            max_files: 2,
                            max_lines: 100,
                            max_symbols: 5,
                            max_destructive_operations: 0,
                            requires_checkpoint: true,
                        },
                        mandatory_verification_per_mutation: true,
                        single_hypothesis_prompting: true,
                    }
                } else {
                    ExecutionStrategy::Guided {
                        blast_budget: BlastBudget {
                            max_files: 3,
                            max_lines: 200,
                            max_symbols: 10,
                            max_destructive_operations: 1,
                            requires_checkpoint: true,
                        },
                        checkpoint_verification: true,
                    }
                }
            }
            CapabilityTier::Medium => {
                if is_high_risk {
                    ExecutionStrategy::Guided {
                        blast_budget: BlastBudget {
                            max_files: 3,
                            max_lines: 250,
                            max_symbols: 12,
                            max_destructive_operations: 1,
                            requires_checkpoint: true,
                        },
                        checkpoint_verification: true,
                    }
                } else {
                    ExecutionStrategy::MacroAutonomous {
                        blast_budget: BlastBudget {
                            max_files: 5,
                            max_lines: 500,
                            max_symbols: 25,
                            max_destructive_operations: 2,
                            requires_checkpoint: false,
                        },
                        phase_level_verification: true,
                    }
                }
            }
            CapabilityTier::Frontier => {
                if is_high_risk {
                    ExecutionStrategy::Guided {
                        blast_budget: BlastBudget {
                            max_files: 5,
                            max_lines: 500,
                            max_symbols: 25,
                            max_destructive_operations: 2,
                            requires_checkpoint: true,
                        },
                        checkpoint_verification: true,
                    }
                } else {
                    ExecutionStrategy::MacroAutonomous {
                        blast_budget: BlastBudget {
                            max_files: 15,
                            max_lines: 1500,
                            max_symbols: 100,
                            max_destructive_operations: 5,
                            requires_checkpoint: false,
                        },
                        phase_level_verification: true,
                    }
                }
            }
        }
    }
}

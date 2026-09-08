use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeComplexity {
    pub symbol_count: usize,
    pub files_count: usize,
    pub dependency_depth: usize,
    pub blast_radius_files: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningComplexity {
    pub ambiguity_score: f32,
    pub architectural_change: bool,
    pub concurrency_risk: bool,
    pub invariant_count: usize,
    pub verification_difficulty: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskComplexity {
    pub code: CodeComplexity,
    pub reasoning: ReasoningComplexity,
}

impl TaskComplexity {
    pub fn is_high_risk(&self) -> bool {
        self.reasoning.architectural_change
            || self.reasoning.concurrency_risk
            || self.reasoning.ambiguity_score > 0.7
            || self.code.blast_radius_files > 5
    }
}

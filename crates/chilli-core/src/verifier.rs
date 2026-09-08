use serde::{Deserialize, Serialize};
use std::path::Path;

pub use chilli_orchestrator::types::{
    LegacyVerificationResult, VerificationEngine, VerificationFailure, VerificationSpec,
    VerificationType,
};
pub use chilli_verification::{
    RawCheckOutput, VerificationExecutor, VerificationInfrastructureError,
};

// --- Async Verification Protocol & Diagnostic Slicing ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureCategory {
    SyntaxError,
    UnresolvedSymbol,
    TypeMismatch,
    TestFailure,
    BuildFailure,
    Timeout,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedDiagnostic {
    pub category: FailureCategory,
    pub message: String,
    pub file_path: Option<String>,
    pub line_number: Option<usize>,
    pub raw_excerpt: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticSlice {
    pub category: FailureCategory,
    pub primary_message: String,
    pub file: Option<String>,
    pub line: Option<usize>,
    pub evidence: Vec<String>,
    pub context_budget_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerificationResult {
    Passed,
    Failed(DiagnosticSlice),
}

pub struct DiagnosticParser;

impl DiagnosticParser {
    pub fn parse(output: &RawCheckOutput) -> Vec<NormalizedDiagnostic> {
        let combined = format!("{}\n{}", output.stdout, output.stderr);
        let mut diagnostics = Vec::new();

        for line in combined.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let category = if trimmed.contains("E0425")
                || trimmed.contains("cannot find value")
                || trimmed.contains("cannot find function")
                || trimmed.contains("cannot find type")
            {
                FailureCategory::UnresolvedSymbol
            } else if trimmed.contains("E0308")
                || trimmed.contains("mismatched types")
                || trimmed.contains("expected ") && trimmed.contains("found ")
            {
                FailureCategory::TypeMismatch
            } else if trimmed.contains("syntax error")
                || trimmed.contains("expected one of")
                || trimmed.contains("parse error")
            {
                FailureCategory::SyntaxError
            } else if trimmed.contains("test ") && trimmed.contains("failed")
                || trimmed.contains("FAILED")
            {
                FailureCategory::TestFailure
            } else if trimmed.contains("error[") || trimmed.contains("could not compile") {
                FailureCategory::BuildFailure
            } else {
                FailureCategory::Unknown
            };

            let (file_path, line_number) = Self::extract_location(trimmed);

            if category != FailureCategory::Unknown || file_path.is_some() {
                diagnostics.push(NormalizedDiagnostic {
                    category,
                    message: trimmed.to_string(),
                    file_path,
                    line_number,
                    raw_excerpt: trimmed.to_string(),
                });
            }
        }

        if diagnostics.is_empty() && !combined.trim().is_empty() {
            diagnostics.push(NormalizedDiagnostic {
                category: FailureCategory::Unknown,
                message: combined
                    .lines()
                    .next()
                    .unwrap_or("Unknown failure")
                    .to_string(),
                file_path: None,
                line_number: None,
                raw_excerpt: combined.lines().take(5).collect::<Vec<_>>().join("\n"),
            });
        }

        diagnostics
    }

    fn extract_location(line: &str) -> (Option<String>, Option<usize>) {
        if let Some(pos) = line.find("--> ") {
            let rest = &line[pos + 4..];
            let parts: Vec<&str> = rest.split(':').collect();
            if !parts.is_empty() {
                let file = parts[0].trim().to_string();
                let line_num = parts.get(1).and_then(|s| s.trim().parse::<usize>().ok());
                return (Some(file), line_num);
            }
        }
        (None, None)
    }
}

pub struct DiagnosticCompressor;

impl DiagnosticCompressor {
    pub fn compress(
        diagnostics: Vec<NormalizedDiagnostic>,
        context_budget_bytes: usize,
    ) -> DiagnosticSlice {
        if diagnostics.is_empty() {
            return DiagnosticSlice {
                category: FailureCategory::Unknown,
                primary_message: "Verification failed without specific diagnostics".to_string(),
                file: None,
                line: None,
                evidence: Vec::new(),
                context_budget_bytes,
            };
        }

        let primary = diagnostics
            .iter()
            .find(|d| d.category != FailureCategory::Unknown)
            .unwrap_or(&diagnostics[0]);

        let category = primary.category;
        let primary_message = primary.message.clone();
        let file = primary.file_path.clone();
        let line = primary.line_number;

        let mut evidence = Vec::new();
        let mut current_bytes = 0;
        let slicing_enabled =
            crate::ABLATION_DIAGNOSTIC_SLICING.load(std::sync::atomic::Ordering::SeqCst);

        for diag in &diagnostics {
            let line_str = &diag.raw_excerpt;
            let line_bytes = line_str.len() + 1;

            if slicing_enabled && current_bytes + line_bytes > context_budget_bytes {
                break;
            }

            evidence.push(line_str.clone());
            current_bytes += line_bytes;
        }

        DiagnosticSlice {
            category,
            primary_message,
            file,
            line,
            evidence,
            context_budget_bytes,
        }
    }
}

pub struct VerificationGate {
    executors: Vec<Box<dyn VerificationExecutor>>,
    context_budget_bytes: usize,
}

impl VerificationGate {
    pub fn new(executors: Vec<Box<dyn VerificationExecutor>>) -> Self {
        Self {
            executors,
            context_budget_bytes: 1024,
        }
    }

    pub fn with_budget(mut self, budget_bytes: usize) -> Self {
        self.context_budget_bytes = budget_bytes;
        self
    }

    pub async fn run_verification(
        &self,
        workspace_root: &Path,
    ) -> Result<VerificationResult, VerificationInfrastructureError> {
        let mut all_diagnostics = Vec::new();
        let mut passed = true;

        for executor in &self.executors {
            let output = executor.execute(workspace_root).await?;
            if output.exit_code != 0 {
                passed = false;
                let parsed = DiagnosticParser::parse(&output);
                all_diagnostics.extend(parsed);
            }
        }

        if passed {
            Ok(VerificationResult::Passed)
        } else {
            let slice = DiagnosticCompressor::compress(all_diagnostics, self.context_budget_bytes);
            Ok(VerificationResult::Failed(slice))
        }
    }
}

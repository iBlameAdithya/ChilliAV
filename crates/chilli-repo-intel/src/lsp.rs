use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum LspSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LspDiagnostic {
    pub file_path: String,
    pub line: usize,
    pub column: usize,
    pub severity: LspSeverity,
    pub message: String,
    pub source: String,
}

pub struct LspDiagnosticClient;

impl LspDiagnosticClient {
    pub fn detect_lsp_provider(workspace_root: &Path) -> &'static str {
        if workspace_root.join("Cargo.toml").exists() {
            "rust-analyzer / cargo check"
        } else if workspace_root.join("tsconfig.json").exists()
            || workspace_root.join("package.json").exists()
        {
            "tsserver / tsc"
        } else if workspace_root.join("pyproject.toml").exists()
            || workspace_root.join("requirements.txt").exists()
        {
            "pyright / python"
        } else {
            "generic ast compiler"
        }
    }

    pub fn collect_rust_diagnostics(workspace_root: &Path) -> Vec<LspDiagnostic> {
        let output = match Command::new("cargo")
            .args(["check", "--message-format=json", "--quiet"])
            .current_dir(workspace_root)
            .output()
        {
            Ok(out) => out,
            Err(_) => return Vec::new(),
        };

        let stdout_str = String::from_utf8_lossy(&output.stdout);
        let mut diagnostics = Vec::new();

        for line in stdout_str.lines() {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
                if val.get("reason").and_then(|r| r.as_str()) == Some("compiler-message") {
                    if let Some(msg) = val.get("message") {
                        let level = msg.get("level").and_then(|l| l.as_str()).unwrap_or("");
                        let severity = match level {
                            "error" => LspSeverity::Error,
                            "warning" => LspSeverity::Warning,
                            "note" => LspSeverity::Information,
                            _ => LspSeverity::Hint,
                        };
                        let message_text = msg
                            .get("message")
                            .and_then(|m| m.as_str())
                            .unwrap_or("")
                            .to_string();

                        if let Some(spans) = msg.get("spans").and_then(|s| s.as_array()) {
                            for span in spans {
                                if span.get("is_primary").and_then(|p| p.as_bool()) == Some(true) {
                                    let rel_path = span
                                        .get("file_name")
                                        .and_then(|f| f.as_str())
                                        .unwrap_or("")
                                        .replace('\\', "/");
                                    let line_start = span
                                        .get("line_start")
                                        .and_then(|l| l.as_u64())
                                        .unwrap_or(1)
                                        as usize;
                                    let col_start = span
                                        .get("column_start")
                                        .and_then(|c| c.as_u64())
                                        .unwrap_or(1)
                                        as usize;

                                    diagnostics.push(LspDiagnostic {
                                        file_path: rel_path,
                                        line: line_start,
                                        column: col_start,
                                        severity,
                                        message: message_text.clone(),
                                        source: "rust-analyzer / cargo check".to_string(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        diagnostics
    }

    pub fn format_diagnostics_for_prompt(diagnostics: &[LspDiagnostic]) -> String {
        if diagnostics.is_empty() {
            return String::new();
        }

        let mut out = String::from("\n? [LSP COMPILER DIAGNOSTICS DETECTED]\n");
        for diag in diagnostics {
            let sev_icon = match diag.severity {
                LspSeverity::Error => "? ERROR",
                LspSeverity::Warning => "?? WARNING",
                LspSeverity::Information => "?? INFO",
                LspSeverity::Hint => "?? HINT",
            };
            out.push_str(&format!(
                "- {} [{}:{}:{}] ({}) {}\n",
                sev_icon, diag.file_path, diag.line, diag.column, diag.source, diag.message
            ));
        }
        out
    }
}

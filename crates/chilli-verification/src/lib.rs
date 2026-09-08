use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RawCheckOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, thiserror::Error)]
pub enum VerificationInfrastructureError {
    #[error("IO error executing verifier: {0}")]
    Io(#[from] std::io::Error),
    #[error("Verification execution timed out")]
    Timeout,
    #[error("Verification execution failed: {0}")]
    ExecutionFailed(String),
}

#[async_trait]
pub trait VerificationExecutor: Send + Sync {
    async fn execute(
        &self,
        workspace_root: &Path,
    ) -> Result<RawCheckOutput, VerificationInfrastructureError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum VerificationLevel {
    FastCheck,
    StandardBuild,
    FullTest,
}

pub trait NamedVerificationExecutor: VerificationExecutor {
    fn name(&self) -> &'static str;
    fn level(&self) -> VerificationLevel;
}

// --- Cargo Verifier ---

pub struct CargoVerifier {
    level: VerificationLevel,
}

impl CargoVerifier {
    pub fn new(level: VerificationLevel) -> Self {
        Self { level }
    }
}

#[async_trait]
impl VerificationExecutor for CargoVerifier {
    async fn execute(
        &self,
        workspace_root: &Path,
    ) -> Result<RawCheckOutput, VerificationInfrastructureError> {
        let args = match self.level {
            VerificationLevel::FastCheck => vec!["check"],
            VerificationLevel::StandardBuild => vec!["build"],
            VerificationLevel::FullTest => vec!["test"],
        };

        let mut cmd = Command::new("cargo");
        cmd.args(&args);
        cmd.current_dir(workspace_root);

        match cmd.output().await {
            Ok(output) => Ok(RawCheckOutput {
                exit_code: output.status.code().unwrap_or(-1),
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            }),
            Err(e) => Ok(RawCheckOutput {
                exit_code: 1,
                stdout: String::new(),
                stderr: format!("Failed to run cargo: {}", e),
            }),
        }
    }
}

impl NamedVerificationExecutor for CargoVerifier {
    fn name(&self) -> &'static str {
        "CargoVerifier"
    }

    fn level(&self) -> VerificationLevel {
        self.level
    }
}

// --- Pytest Verifier ---

pub struct PytestVerifier {
    level: VerificationLevel,
}

impl PytestVerifier {
    pub fn new(level: VerificationLevel) -> Self {
        Self { level }
    }
}

#[async_trait]
impl VerificationExecutor for PytestVerifier {
    async fn execute(
        &self,
        workspace_root: &Path,
    ) -> Result<RawCheckOutput, VerificationInfrastructureError> {
        let (cmd_name, args) = match self.level {
            VerificationLevel::FastCheck => ("python", vec!["-m", "pytest", "--collect-only"]),
            VerificationLevel::StandardBuild => ("python", vec!["-m", "py_compile"]),
            VerificationLevel::FullTest => ("pytest", vec![]),
        };

        let mut cmd = Command::new(cmd_name);
        cmd.args(&args);
        cmd.current_dir(workspace_root);

        match cmd.output().await {
            Ok(output) => Ok(RawCheckOutput {
                exit_code: output.status.code().unwrap_or(-1),
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            }),
            Err(e) => Ok(RawCheckOutput {
                exit_code: 1,
                stdout: String::new(),
                stderr: format!("Failed to run python/pytest: {}", e),
            }),
        }
    }
}

impl NamedVerificationExecutor for PytestVerifier {
    fn name(&self) -> &'static str {
        "PytestVerifier"
    }

    fn level(&self) -> VerificationLevel {
        self.level
    }
}

// --- TypeScript Verifier ---

pub struct TypeScriptVerifier {
    level: VerificationLevel,
}

impl TypeScriptVerifier {
    pub fn new(level: VerificationLevel) -> Self {
        Self { level }
    }
}

#[async_trait]
impl VerificationExecutor for TypeScriptVerifier {
    async fn execute(
        &self,
        workspace_root: &Path,
    ) -> Result<RawCheckOutput, VerificationInfrastructureError> {
        let (cmd_name, args) = match self.level {
            VerificationLevel::FastCheck => ("npx", vec!["tsc", "--noEmit"]),
            VerificationLevel::StandardBuild => ("npm", vec!["run", "build"]),
            VerificationLevel::FullTest => ("npm", vec!["test"]),
        };

        let mut cmd = Command::new(cmd_name);
        cmd.args(&args);
        cmd.current_dir(workspace_root);

        match cmd.output().await {
            Ok(output) => Ok(RawCheckOutput {
                exit_code: output.status.code().unwrap_or(-1),
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            }),
            Err(e) => Ok(RawCheckOutput {
                exit_code: 1,
                stdout: String::new(),
                stderr: format!("Failed to run npm/tsc: {}", e),
            }),
        }
    }
}

impl NamedVerificationExecutor for TypeScriptVerifier {
    fn name(&self) -> &'static str {
        "TypeScriptVerifier"
    }

    fn level(&self) -> VerificationLevel {
        self.level
    }
}

/// Injects failing test function source code and assertion logic into repair prompts
/// so model does not fix code blindly without seeing test assertions.
pub fn extract_failing_test_assertion(
    workspace_root: &Path,
    traceback: &str,
) -> Option<(String, String)> {
    for line in traceback.lines() {
        if line.contains("File \"") && (line.contains("test") || line.contains("tests/")) {
            if let Some(start) = line.find("File \"") {
                let rest = &line[start + 6..];
                if let Some(end) = rest.find('"') {
                    let rel_path = rest[..end].replace('\\', "/");
                    let full_path = workspace_root.join(&rel_path);
                    if full_path.exists() {
                        if let Ok(content) = std::fs::read_to_string(&full_path) {
                            let lines: Vec<&str> = content.lines().collect();
                            let test_slice = lines[..lines.len().min(80)].join("\n");
                            return Some((rel_path, test_slice));
                        }
                    }
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_failing_test_assertion() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path();
        let test_file = root.join("tests/test_admin.py");
        std::fs::create_dir_all(test_file.parent().unwrap()).unwrap();
        std::fs::write(&test_file, "def test_ordering(): assert False\n").unwrap();

        let tb = "Traceback:\n  File \"tests/test_admin.py\", line 1, in test_ordering\n    assert False";
        let res = extract_failing_test_assertion(root, tb);
        assert!(res.is_some());
        let (rel, code) = res.unwrap();
        assert_eq!(rel, "tests/test_admin.py");
        assert!(code.contains("def test_ordering():"));
    }
}

// --- Factory ---

pub struct LanguageVerifierFactory;

impl LanguageVerifierFactory {
    pub fn detect_verifiers(
        workspace_root: &Path,
        level: VerificationLevel,
    ) -> Vec<Box<dyn NamedVerificationExecutor>> {
        let mut verifiers: Vec<Box<dyn NamedVerificationExecutor>> = Vec::new();

        if workspace_root.join("Cargo.toml").exists() {
            verifiers.push(Box::new(CargoVerifier::new(level)));
        }

        if workspace_root.join("pyproject.toml").exists()
            || workspace_root.join("pytest.ini").exists()
            || workspace_root.join("setup.py").exists()
            || workspace_root.join("requirements.txt").exists()
            || workspace_root.join("tests").is_dir()
            || workspace_root.join("test").is_dir()
        {
            verifiers.push(Box::new(PytestVerifier::new(level)));
        }

        if workspace_root.join("package.json").exists()
            || workspace_root.join("tsconfig.json").exists()
        {
            verifiers.push(Box::new(TypeScriptVerifier::new(level)));
        }

        verifiers
    }
}

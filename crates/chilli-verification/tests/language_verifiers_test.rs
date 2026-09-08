use chilli_verification::{
    CargoVerifier, LanguageVerifierFactory, PytestVerifier, TypeScriptVerifier,
    VerificationExecutor, VerificationLevel,
};
use std::fs;
use tempfile::tempdir;

#[tokio::test]
async fn test_cargo_verifier_fast_check_executes_on_temp_dir() {
    let dir = tempdir().unwrap();
    let cargo_toml = dir.path().join("Cargo.toml");
    fs::write(
        &cargo_toml,
        r#"[package]
name = "test-pkg"
version = "0.1.0"
edition = "2021"
"#,
    )
    .unwrap();

    let verifier = CargoVerifier::new(VerificationLevel::FastCheck);
    let result = verifier.execute(dir.path()).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_pytest_verifier_executes_on_temp_dir() {
    let dir = tempdir().unwrap();
    let verifier = PytestVerifier::new(VerificationLevel::FastCheck);
    let result = verifier.execute(dir.path()).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_typescript_verifier_executes_on_temp_dir() {
    let dir = tempdir().unwrap();
    let verifier = TypeScriptVerifier::new(VerificationLevel::FastCheck);
    let result = verifier.execute(dir.path()).await;
    assert!(result.is_ok());
}

#[test]
fn test_factory_detects_rust_project() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();

    let verifiers =
        LanguageVerifierFactory::detect_verifiers(dir.path(), VerificationLevel::FastCheck);
    assert_eq!(verifiers.len(), 1);
    assert_eq!(verifiers[0].name(), "CargoVerifier");
}

#[test]
fn test_factory_detects_python_project() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("pyproject.toml"), "[tool.pytest]").unwrap();

    let verifiers =
        LanguageVerifierFactory::detect_verifiers(dir.path(), VerificationLevel::FullTest);
    assert_eq!(verifiers.len(), 1);
    assert_eq!(verifiers[0].name(), "PytestVerifier");
}

#[test]
fn test_factory_detects_typescript_project() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("package.json"), "{}").unwrap();

    let verifiers =
        LanguageVerifierFactory::detect_verifiers(dir.path(), VerificationLevel::FastCheck);
    assert_eq!(verifiers.len(), 1);
    assert_eq!(verifiers[0].name(), "TypeScriptVerifier");
}

#[tokio::test]
async fn test_dle_repository_verifier_detection_and_execution() {
    let dle_path = std::path::Path::new(r"C:\Users\Nakul Adithya\OneDrive\Documents\DLE");
    if !dle_path.exists() {
        return;
    }

    let verifiers =
        LanguageVerifierFactory::detect_verifiers(dle_path, VerificationLevel::FastCheck);
    assert!(
        !verifiers.is_empty(),
        "Expected DLE repository to be detected as Python project"
    );
    assert_eq!(verifiers[0].name(), "PytestVerifier");

    let result = verifiers[0].execute(dle_path).await;
    assert!(result.is_ok());
}

use chilli_core::verifier::{VerificationEngine, VerificationSpec, VerificationType};

#[test]
fn test_verification_engine_file_content_and_command() {
    let ws = std::env::temp_dir().join("chilli_verifier_test");
    let _ = std::fs::remove_dir_all(&ws);
    std::fs::create_dir_all(&ws).unwrap();

    std::fs::write(ws.join("output.txt"), "hello world").unwrap();

    let engine = VerificationEngine::new(&ws);
    let spec = VerificationSpec {
        checks: vec![
            VerificationType::FileContains {
                path: "output.txt".to_string(),
                expected: "hello".to_string(),
            },
            VerificationType::CommandExitZero {
                command: "cargo".to_string(),
                args: vec!["--version".to_string()],
            },
        ],
    };

    let res = engine.verify(&spec);
    assert!(res.passed);
    assert_eq!(res.failures.len(), 0);
}

#[test]
fn test_verification_engine_failure_cases() {
    let ws = std::env::temp_dir().join("chilli_verifier_fail_test");
    let _ = std::fs::remove_dir_all(&ws);
    std::fs::create_dir_all(&ws).unwrap();

    std::fs::write(ws.join("output.txt"), "goodbye world").unwrap();

    let engine = VerificationEngine::new(&ws);
    let spec = VerificationSpec {
        checks: vec![VerificationType::FileContains {
            path: "output.txt".to_string(),
            expected: "hello".to_string(),
        }],
    };

    let res = engine.verify(&spec);
    assert!(!res.passed);
    assert_eq!(res.failures.len(), 1);
}

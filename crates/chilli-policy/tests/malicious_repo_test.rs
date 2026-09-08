use chilli_policy::command_policy::evaluate_command_tokens;
use chilli_policy::path_policy::{PathPolicy, PolicyDecision};
use std::fs;
use tempfile::tempdir;

#[test]
fn test_malicious_repo_path_isolation() {
    let temp_workspace = tempdir().unwrap();
    let workspace_root = temp_workspace.path().canonicalize().unwrap();

    let policy = PathPolicy;

    // 1. Hidden .env file harvester access denial
    let env_file = workspace_root.join(".env");
    fs::write(&env_file, "SECRET_KEY=super_secret_payload").unwrap();
    assert_eq!(
        policy.check_read(&workspace_root, &env_file),
        PolicyDecision::Deny("Sensitive path access forbidden".to_string())
    );

    // 2. Sensitive directory access denial (.aws, .ssh)
    let aws_creds = workspace_root.join(".aws/credentials");
    assert_eq!(
        policy.check_read(&workspace_root, &aws_creds),
        PolicyDecision::Deny("Sensitive path access forbidden".to_string())
    );

    // 3. Path traversal attack via malicious filenames inside repo
    let evil_filename = workspace_root.join("subfolder/../../../../etc/passwd");
    assert_eq!(
        policy.check_read(&workspace_root, &evil_filename),
        PolicyDecision::Deny("Sensitive path access forbidden".to_string())
    );
}

#[test]
fn test_malicious_repo_command_injection_defense() {
    // 1. Obfuscated rm -rf with extra spaces and split flags
    let cmd1 = "rm   -r   -f   /tmp/data";
    assert!(matches!(
        evaluate_command_tokens(cmd1),
        PolicyDecision::Deny(_)
    ));

    // 2. Chained command with semicolon & destructive tail
    let cmd2 = "cargo check ; rm -rf .";
    assert!(matches!(
        evaluate_command_tokens(cmd2),
        PolicyDecision::Deny(_)
    ));

    // 3. Git force push attempt in script
    let cmd3 = "git push origin main -f";
    assert!(matches!(
        evaluate_command_tokens(cmd3),
        PolicyDecision::Deny(_)
    ));
}

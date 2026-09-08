use chilli_policy::credential_filter::mask_credentials;

#[test]
fn test_credential_leakage_masking() {
    // 1. Anthropic API key masking
    let text = "My key is sk-ant-api03-1234567890abcdef1234567890abcdef-AA";
    let masked = mask_credentials(text);
    assert!(!masked.contains("sk-ant-"));
    assert!(masked.contains("[REDACTED_ANTHROPIC_KEY]"));

    // 2. OpenAI API key masking
    let text2 = "OpenAI key: sk-abcdef1234567890abcdef1234567890";
    let masked2 = mask_credentials(text2);
    assert!(!masked2.contains("sk-abcdef"));
    assert!(masked2.contains("[REDACTED_API_KEY]"));

    // 3. GitHub Personal Access Token masking
    let text3 = "Token ghp_1234567890abcdef1234567890abcdef123456";
    let masked3 = mask_credentials(text3);
    assert!(!masked3.contains("ghp_1234567890"));
    assert!(masked3.contains("[REDACTED_GITHUB_PAT]"));

    // 4. AWS Secret Access Key masking
    let text4 = "aws_secret_access_key = wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
    let masked4 = mask_credentials(text4);
    assert!(!masked4.contains("EXAMPLEKEY"));
    assert!(masked4.contains("[REDACTED_AWS_SECRET]"));

    // 5. RSA Private key block masking
    let text5 = "-----BEGIN PRIVATE KEY-----\nMIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQC...\n-----END PRIVATE KEY-----";
    let masked5 = mask_credentials(text5);
    assert!(!masked5.contains("BEGIN PRIVATE KEY"));
    assert!(masked5.contains("[REDACTED_PRIVATE_KEY]"));
}

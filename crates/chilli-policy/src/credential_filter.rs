use regex::Regex;
use std::sync::OnceLock;

static PATTERNS: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();

fn get_patterns() -> &'static Vec<(Regex, &'static str)> {
    PATTERNS.get_or_init(|| {
        vec![
            // OpenAI / Anthropic / Groq API keys
            (Regex::new(r"(?i)gsk_[a-zA-Z0-9_\-]{20,}").unwrap(), "[REDACTED_GROQ_KEY]"),
            (Regex::new(r"(?i)sk-ant-[a-zA-Z0-9_\-]{20,}").unwrap(), "[REDACTED_ANTHROPIC_KEY]"),
            (Regex::new(r"(?i)sk-[a-zA-Z0-9_\-]{20,}").unwrap(), "[REDACTED_API_KEY]"),

            // GitHub Tokens
            (Regex::new(r"(?i)ghp_[a-zA-Z0-9]{36}").unwrap(), "[REDACTED_GITHUB_PAT]"),
            (Regex::new(r"(?i)gho_[a-zA-Z0-9]{36}").unwrap(), "[REDACTED_GITHUB_OAUTH]"),
            (Regex::new(r"(?i)ghr_[a-zA-Z0-9]{36}").unwrap(), "[REDACTED_GITHUB_REFRESH]"),
            (Regex::new(r"(?i)github_pat_[a-zA-Z0-9_]{22,}").unwrap(), "[REDACTED_GITHUB_PAT]"),

            // AWS Keys
            (Regex::new(r"(?i)\bAKIA[0-9A-Z]{16}\b").unwrap(), "[REDACTED_AWS_KEY_ID]"),
            (Regex::new(r#"(?i)(aws_secret_access_key|aws_secret_key)\s*=\s*['"]?[A-Za-z0-9/+=]{40}['"]?"#).unwrap(), "$1=[REDACTED_AWS_SECRET]"),

            // Private Keys
            (Regex::new(r"(?s)-----BEGIN [A-Z ]*PRIVATE KEY-----.*?-----END [A-Z ]*PRIVATE KEY-----").unwrap(), "[REDACTED_PRIVATE_KEY]"),

            // Generic Authorization headers
            (Regex::new(r"(?i)(Authorization:\s*Bearer\s+)[A-Za-z0-9_\-\.=]{20,}").unwrap(), "$1[REDACTED_BEARER_TOKEN]"),

            // Env assignment patterns: key=val or token=val
            (Regex::new(r#"(?i)(api_key|secret|password|passwd|token|auth_token)\s*=\s*['"]?[^\s'"]{8,}['"]?"#).unwrap(), "$1=[REDACTED_SECRET]"),
        ]
    })
}

/// Redact credentials and API keys from arbitrary text string.
pub fn mask_credentials(input: &str) -> String {
    let mut result = input.to_string();
    for (re, replacement) in get_patterns() {
        result = re.replace_all(&result, *replacement).to_string();
    }
    result
}

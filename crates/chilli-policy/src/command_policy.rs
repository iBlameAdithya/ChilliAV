use crate::path_policy::{contains_sensitive_pattern, PolicyDecision};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Word(String),
    Operator(String),
}

/// Tokenize shell command line into words and operators.
/// Handles single quotes, double quotes, escape characters, and operators (;, &&, ||, |, &).
pub fn tokenize_command(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    while let Some(ch) = chars.next() {
        if in_single_quote {
            if ch == '\'' {
                in_single_quote = false;
            } else {
                current.push(ch);
            }
            continue;
        }

        if in_double_quote {
            if ch == '"' {
                in_double_quote = false;
            } else if ch == '\\' {
                if let Some(&next_ch) = chars.peek() {
                    if next_ch == '"' || next_ch == '\\' || next_ch == '$' || next_ch == '`' {
                        current.push(chars.next().unwrap());
                    } else {
                        current.push(ch);
                    }
                } else {
                    current.push(ch);
                }
            } else {
                current.push(ch);
            }
            continue;
        }

        match ch {
            '\'' => {
                in_single_quote = true;
            }
            '"' => {
                in_double_quote = true;
            }
            '\\' => {
                if let Some(next_ch) = chars.next() {
                    current.push(next_ch);
                }
            }
            ';' | '|' | '&' => {
                if !current.trim().is_empty() {
                    tokens.push(Token::Word(current.trim().to_string()));
                    current.clear();
                }
                let mut op = ch.to_string();
                if let Some(&next_ch) = chars.peek() {
                    if (ch == '&' && next_ch == '&') || (ch == '|' && next_ch == '|') {
                        op.push(chars.next().unwrap());
                    }
                }
                tokens.push(Token::Operator(op));
            }
            c if c.is_whitespace() => {
                if !current.trim().is_empty() {
                    tokens.push(Token::Word(current.trim().to_string()));
                    current.clear();
                }
            }
            _ => {
                current.push(ch);
            }
        }
    }

    if !current.trim().is_empty() {
        tokens.push(Token::Word(current.trim().to_string()));
    }

    tokens
}

/// Splits tokens by operators into individual commands.
pub fn split_into_subcommands(tokens: &[Token]) -> Vec<Vec<String>> {
    let mut subcommands = Vec::new();
    let mut current_cmd = Vec::new();

    for token in tokens {
        match token {
            Token::Word(word) => {
                current_cmd.push(word.clone());
            }
            Token::Operator(_) => {
                if !current_cmd.is_empty() {
                    subcommands.push(current_cmd.clone());
                    current_cmd.clear();
                }
            }
        }
    }
    if !current_cmd.is_empty() {
        subcommands.push(current_cmd);
    }
    subcommands
}

/// Normalizes flags e.g. "-rf" -> ["-r", "-f"], "--force" -> ["--force"].
pub fn expand_flags(word: &str) -> Vec<String> {
    if word.starts_with("--") {
        vec![word.to_string()]
    } else if word.starts_with('-') && word.len() > 2 {
        word.chars().skip(1).map(|c| format!("-{}", c)).collect()
    } else {
        vec![word.to_string()]
    }
}

/// Check parsed command against tokenized policy rules.
pub fn evaluate_command_tokens(command_line: &str) -> PolicyDecision {
    let tokens = tokenize_command(command_line);
    let subcommands = split_into_subcommands(&tokens);

    if subcommands.is_empty() {
        return PolicyDecision::Allow;
    }

    for cmd in subcommands {
        if cmd.is_empty() {
            continue;
        }

        let binary = cmd[0].to_lowercase();
        let binary_name = std::path::Path::new(&binary)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&binary);

        // Check each word for sensitive pattern access
        for arg in &cmd {
            if let Some(pattern) = contains_sensitive_pattern(arg) {
                return PolicyDecision::Deny(format!(
                    "Command argument '{}' contains forbidden sensitive pattern '{}'",
                    arg, pattern
                ));
            }
        }

        // Expand all flags in arguments for tokenized flag matching
        let mut expanded_args = Vec::new();
        for arg in &cmd[1..] {
            if arg.starts_with('-') && !arg.starts_with("--") {
                expanded_args.extend(expand_flags(arg));
            } else {
                expanded_args.push(arg.clone());
            }
        }

        // 1. Destructive rm check: rm -r -f or rm -rf
        if binary_name == "rm" || binary_name == "rm.exe" {
            let has_r = expanded_args
                .iter()
                .any(|a| a == "-r" || a == "-R" || a == "--recursive");
            let has_f = expanded_args.iter().any(|a| a == "-f" || a == "--force");
            if has_r && has_f {
                return PolicyDecision::Deny(
                    "Recursive forced deletion ('rm -rf') forbidden".to_string(),
                );
            }
        }

        // 2. Destructive rmdir check: rmdir /s
        if (binary_name == "rmdir" || binary_name == "rmdir.exe")
            && expanded_args
                .iter()
                .any(|a| a.to_lowercase() == "/s" || a.to_lowercase() == "-s")
        {
            return PolicyDecision::Deny(
                "Recursive directory removal ('rmdir /s') forbidden".to_string(),
            );
        }

        // 3. Git force push check: git push --force / -f
        if binary_name == "git" || binary_name == "git.exe" {
            let is_push = cmd.get(1).map(|s| s.as_str()) == Some("push");
            if is_push {
                let has_force = expanded_args
                    .iter()
                    .any(|a| a == "-f" || a == "--force" || a == "--force-with-lease");
                if has_force {
                    return PolicyDecision::Deny(
                        "Forced git push ('git push --force') forbidden".to_string(),
                    );
                }
            }
        }
    }

    PolicyDecision::Allow
}

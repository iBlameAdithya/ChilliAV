use std::collections::BTreeSet;

use crate::types::{estimate_tokens, MessageItem, Role};

pub struct Compactor;

impl Compactor {
    /// Compacts messages when total estimated tokens exceed threshold (e.g. 80% of max window).
    /// Keeps user task (first message if user task) and recent N turns, compressing middle turns
    /// into a structured milestone summary message.
    pub fn compact_messages(
        messages: &[MessageItem],
        max_window_tokens: usize,
    ) -> Vec<MessageItem> {
        Self::compact_messages_with_checkpoint(messages, max_window_tokens, None)
    }

    pub fn compact_messages_with_checkpoint(
        messages: &[MessageItem],
        max_window_tokens: usize,
        checkpoint_summary: Option<&str>,
    ) -> Vec<MessageItem> {
        let threshold = (max_window_tokens as f64 * 0.8) as usize;
        let total_tokens: usize = messages.iter().map(|m| estimate_tokens(&m.content)).sum();

        if total_tokens <= threshold || messages.len() <= 4 {
            return messages.to_vec();
        }

        let mut compacted = Vec::new();

        // Preserve first message (typically User task)
        let first_msg = messages[0].clone();
        compacted.push(first_msg);

        // Keep last 4 messages intact
        let keep_recent_count = 4;
        let middle_end = messages.len().saturating_sub(keep_recent_count);

        if middle_end > 1 {
            let middle_slice = &messages[1..middle_end];
            let mut structured_summary = Self::build_structured_milestone(middle_slice);
            if let Some(chk) = checkpoint_summary {
                structured_summary.push_str("\n- Checkpoint State: ");
                structured_summary.push_str(chk);
            }

            compacted.push(MessageItem {
                role: Role::User,
                content: structured_summary,
                name: None,
                tool_calls: None,
                tool_call_id: None,
            });
        }

        // Append recent messages
        for msg in &messages[middle_end..] {
            compacted.push(msg.clone());
        }

        compacted
    }

    /// Extracts structured milestone details from middle-turn dialogue.
    pub fn build_structured_milestone(middle_messages: &[MessageItem]) -> String {
        let mut modified_files = BTreeSet::new();
        let mut tool_actions = Vec::new();
        let mut test_statuses = Vec::new();
        let mut verification_failures = Vec::new();

        for msg in middle_messages {
            let content = &msg.content;

            // Extract file paths
            for word in content.split_whitespace() {
                let clean_word = word.trim_matches(|c| {
                    c == '"'
                        || c == '\''
                        || c == '`'
                        || c == ','
                        || c == ':'
                        || c == '('
                        || c == ')'
                });
                if (clean_word.contains("src/")
                    || clean_word.contains("crates/")
                    || clean_word.contains("tests/")
                    || clean_word.contains("benches/")
                    || clean_word.ends_with(".rs")
                    || clean_word.ends_with(".toml")
                    || clean_word.ends_with(".json")
                    || clean_word.ends_with(".md")
                    || clean_word.ends_with(".py")
                    || clean_word.ends_with(".js")
                    || clean_word.ends_with(".ts"))
                    && clean_word.len() > 3
                {
                    modified_files.insert(clean_word.to_string());
                }
            }

            // Extract tool actions
            if content.contains("write_file") || content.contains("edit_file") {
                tool_actions.push("File modification executed");
            } else if content.contains("exec")
                || content.contains("cargo test")
                || content.contains("cargo build")
            {
                tool_actions.push("Command execution");
            }

            // Extract test / verification status
            if content.contains("Verification check failed")
                || content.contains("Verification failed:")
            {
                let failure_line = content
                    .lines()
                    .find(|l| l.contains("failed") || l.contains("Failure"))
                    .unwrap_or("Verification failure");
                verification_failures.push(failure_line.to_string());
                test_statuses.push("Verification failure recorded");
            } else if content.contains("test result: ok") || content.contains("passed;") {
                test_statuses.push("Tests passing");
            } else if content.contains("FAILED") || content.contains("failed;") {
                test_statuses.push("Test failure recorded");
            }
        }

        let files_str = if modified_files.is_empty() {
            "None recorded".to_string()
        } else {
            modified_files.into_iter().collect::<Vec<_>>().join(", ")
        };

        let actions_count = tool_actions.len().max(middle_messages.len());
        let status_summary = test_statuses.last().copied().unwrap_or("In progress");
        let failures_summary = if verification_failures.is_empty() {
            "None".to_string()
        } else {
            verification_failures.join("; ")
        };

        format!(
            "[Structured Milestone Summary]\n- Compacted Turns: {}\n- Modified Files: [{}]\n- Tool Actions Recorded: {}\n- Last Verification Status: {}\n- Recent Failures Preserved: {}",
            middle_messages.len(),
            files_str,
            actions_count,
            status_summary,
            failures_summary
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compactor_no_op_below_threshold() {
        let msgs = vec![
            MessageItem {
                role: Role::User,
                content: "Task".into(),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
            MessageItem {
                role: Role::Assistant,
                content: "Reply".into(),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
        ];
        let result = Compactor::compact_messages(&msgs, 100_000);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_compactor_compacts_when_exceeding_threshold() {
        let mut msgs = vec![MessageItem {
            role: Role::User,
            content: "Task instructions".into(),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        }];
        for i in 0..10 {
            msgs.push(MessageItem {
                role: Role::Assistant,
                content: format!("Large output content step {} for src/main.rs", i).repeat(20),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            });
            msgs.push(MessageItem {
                role: Role::User,
                content: format!("Tool observation output {}", i).repeat(20),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            });
        }

        let result = Compactor::compact_messages(&msgs, 500);
        assert!(result
            .iter()
            .any(|m| m.content.contains("[Structured Milestone Summary]")));
        assert!(result.iter().any(|m| m.content.contains("src/main.rs")));
        assert_eq!(result[0].content, "Task instructions");
    }

    #[test]
    fn test_structured_milestone_compaction() {
        let messages = [
            MessageItem {
                role: Role::User,
                content: "Fix bug in src/lib.rs".into(),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
            MessageItem {
                role: Role::Assistant,
                content: "Executing write_file for src/lib.rs".into(),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
            MessageItem {
                role: Role::User,
                content: "Observation: File written successfully".into(),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
            MessageItem {
                role: Role::Assistant,
                content: "Executing exec cargo test".into(),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
            MessageItem {
                role: Role::User,
                content: "Observation: test result: ok. 5 passed;".into(),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
        ];
        let summary = Compactor::build_structured_milestone(&messages[1..4]);
        assert!(summary.contains("[Structured Milestone Summary]"));
        assert!(summary.contains("src/lib.rs"));
        assert!(summary.contains("Compacted Turns: 3"));
    }
}

use crate::types::estimate_tokens;

pub struct ObservationTruncator;

impl ObservationTruncator {
    /// Truncates large tool observation output keeping first `head_lines` and last `tail_lines`
    /// when total tokens exceed `max_tokens` or line count exceeds limit.
    pub fn truncate(output: &str, max_tokens: usize) -> String {
        let current_tokens = estimate_tokens(output);
        if current_tokens <= max_tokens {
            return output.to_string();
        }

        let lines: Vec<&str> = output.lines().collect();
        if lines.len() <= 20 {
            // Short line count but long lines: char truncation
            let char_limit = max_tokens * 4;
            if output.len() > char_limit {
                let half = char_limit / 2;
                let head_boundary = output
                    .char_indices()
                    .map(|(i, _)| i)
                    .take_while(|&i| i <= half)
                    .last()
                    .unwrap_or(0);
                let tail_target = output.len().saturating_sub(half);
                let tail_boundary = output
                    .char_indices()
                    .map(|(i, _)| i)
                    .find(|&i| i >= tail_target)
                    .unwrap_or(output.len());

                let head = &output[..head_boundary];
                let tail = &output[tail_boundary..];
                return format!("{}\n[Truncated long text...]\n{}", head, tail);
            }
            return output.to_string();
        }

        let head_count = 10;
        let tail_count = 10;

        if lines.len() <= head_count + tail_count {
            return output.to_string();
        }

        let head_part = lines[..head_count].join("\n");
        let tail_part = lines[lines.len() - tail_count..].join("\n");
        let truncated_count = lines.len() - (head_count + tail_count);

        format!(
            "{}\n\n[Truncated {} lines...]\n\n{}",
            head_part, truncated_count, tail_part
        )
    }

    /// Specializes truncation logic deterministically based on tool identity.
    pub fn truncate_tool_output(tool_name: &str, output: &str, max_tokens: usize) -> String {
        let current_tokens = estimate_tokens(output);
        if current_tokens <= max_tokens {
            return output.to_string();
        }

        match tool_name {
            "exec" | "run_command" | "test_runner" => {
                let lines: Vec<&str> = output.lines().collect();
                let mut failure_lines = Vec::new();

                for line in &lines {
                    let is_error_signal = line.contains("panicked at")
                        || line.contains("FAIL:")
                        || line.contains("failures:")
                        || line.contains("test result:")
                        || line.contains("Error")
                        || line.contains("stack backtrace:");

                    if is_error_signal {
                        failure_lines.push(*line);
                    }
                }

                if !failure_lines.is_empty() {
                    let head_count = 5.min(lines.len());
                    let head_part = lines[..head_count].join("\n");
                    let error_part = failure_lines.join("\n");
                    let tail_count = 5.min(lines.len());
                    let tail_part = lines[lines.len() - tail_count..].join("\n");

                    format!(
                        "{}\n\n[Preserving Error Context]\n{}\n\n[Truncated Execution Output...]\n{}",
                        head_part, error_part, tail_part
                    )
                } else {
                    Self::truncate(output, max_tokens)
                }
            }
            "grep" | "search" => {
                let lines: Vec<&str> = output.lines().collect();
                if lines.len() > 30 {
                    let kept = lines[..30].join("\n");
                    let truncated_count = lines.len() - 30;
                    format!(
                        "{}\n\n[Truncated {} additional matching lines (total: {} matches)]",
                        kept,
                        truncated_count,
                        lines.len()
                    )
                } else {
                    Self::truncate(output, max_tokens)
                }
            }
            "read_file" => {
                let lines: Vec<&str> = output.lines().collect();
                if lines.len() > 40 {
                    let head_part = lines[..20].join("\n");
                    let tail_part = lines[lines.len() - 20..].join("\n");
                    let truncated_count = lines.len() - 40;
                    format!(
                        "[Lines 1-20 and {}-{} of {}]\n{}\n\n[Truncated {} lines...]\n\n{}",
                        lines.len() - 20 + 1,
                        lines.len(),
                        lines.len(),
                        head_part,
                        truncated_count,
                        tail_part
                    )
                } else {
                    Self::truncate(output, max_tokens)
                }
            }
            _ => Self::truncate(output, max_tokens),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncator_preserves_short_log() {
        let log = "line 1\nline 2\nline 3";
        let result = ObservationTruncator::truncate(log, 1000);
        assert_eq!(result, log);
    }

    #[test]
    fn test_truncator_truncates_large_log() {
        let log = "log line\n".repeat(500);
        let result = ObservationTruncator::truncate(&log, 100);
        assert!(result.contains("[Truncated"));
        assert!(result.contains("lines...]"));
    }

    #[test]
    fn test_tool_aware_truncation_exec_panic_preservation() {
        let output = "running 50 tests\ntest test_1 ... ok\ntest test_2 ... FAILED\n\nfailures:\n\n---- test_2 stdout ----\nthread 'test_2' panicked at 'assertion failed: left == right', src/lib.rs:42:5\nstack backtrace:\n   0: std::sys::backtrace\n   1: main\n";
        let truncated = ObservationTruncator::truncate_tool_output("exec", output, 10);
        assert!(truncated.contains("panicked at"));
        assert!(truncated.contains("src/lib.rs:42:5"));
    }

    #[test]
    fn test_tool_aware_truncation_grep_formatting() {
        let grep_output = "src/main.rs:1: fn main()\n".repeat(50);
        let truncated = ObservationTruncator::truncate_tool_output("grep", &grep_output, 10);
        assert!(truncated.contains("[Truncated 20 additional matching lines (total: 50 matches)]"));
    }

    #[test]
    fn test_tool_aware_truncation_read_file_formatting() {
        let file_content = "let x = 1;\n".repeat(100);
        let truncated = ObservationTruncator::truncate_tool_output("read_file", &file_content, 10);
        assert!(truncated.contains("[Lines 1-20 and 81-100 of 100]"));
    }
}

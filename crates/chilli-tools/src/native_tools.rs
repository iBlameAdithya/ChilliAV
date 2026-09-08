use chilli_policy::credential_filter::mask_credentials;
use chilli_policy::path_policy::{PathPolicy, PolicyDecision};
use chilli_sandbox::sandbox::SandboxCapabilities;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub output: String,
}

pub trait Tool {
    fn name(&self) -> &str;
    fn execute(&self, args: serde_json::Value) -> Result<ToolResult, String>;
}

pub struct ReadFileTool {
    workspace_root: PathBuf,
    policy: PathPolicy,
}

impl ReadFileTool {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self {
            workspace_root,
            policy: PathPolicy,
        }
    }
}

impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn execute(&self, args: serde_json::Value) -> Result<ToolResult, String> {
        let rel_path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing required parameter 'path'".to_string())?;

        let target_path = self.workspace_root.join(rel_path);

        if let PolicyDecision::Deny(reason) =
            self.policy.check_read(&self.workspace_root, &target_path)
        {
            return Ok(ToolResult {
                success: false,
                output: format!("Permission denied: {}", reason),
            });
        }

        if !target_path.exists() {
            return Ok(ToolResult {
                success: false,
                output: format!(
                    "File '{}' does not exist. If you want to create a new file, use write_file.",
                    rel_path
                ),
            });
        }

        if target_path.is_dir() {
            return Ok(ToolResult {
                success: false,
                output: format!(
                    "Path '{}' is a directory. To list directory contents, use list_directory.",
                    rel_path
                ),
            });
        }

        let content = fs::read_to_string(&target_path)
            .map_err(|e| format!("Failed to read file '{}': {}", rel_path, e))?;

        let lines: Vec<&str> = content.lines().collect();
        let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
        let limit = args
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(lines.len());

        let start = if offset > 0 { offset - 1 } else { 0 };
        let end = (start + limit).min(lines.len());

        if start >= lines.len() {
            return Ok(ToolResult {
                success: true,
                output: String::new(),
            });
        }

        let selected = lines[start..end].join("\n");
        Ok(ToolResult {
            success: true,
            output: mask_credentials(&selected),
        })
    }
}

pub struct WriteFileTool {
    workspace_root: PathBuf,
    policy: PathPolicy,
}

impl WriteFileTool {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self {
            workspace_root,
            policy: PathPolicy,
        }
    }
}

impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn execute(&self, args: serde_json::Value) -> Result<ToolResult, String> {
        let rel_path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing required parameter 'path'".to_string())?;

        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing required parameter 'content'".to_string())?;

        let target_path = self.workspace_root.join(rel_path);

        if let PolicyDecision::Deny(reason) =
            self.policy.check_write(&self.workspace_root, &target_path)
        {
            return Ok(ToolResult {
                success: false,
                output: format!("Permission denied: {}", reason),
            });
        }

        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create parent directories: {}", e))?;
        }

        fs::write(&target_path, content)
            .map_err(|e| format!("Failed to write file '{}': {}", rel_path, e))?;

        Ok(ToolResult {
            success: true,
            output: format!("File successfully written to {}", rel_path),
        })
    }
}

pub struct EditFileTool {
    workspace_root: PathBuf,
    policy: PathPolicy,
}

impl EditFileTool {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self {
            workspace_root,
            policy: PathPolicy,
        }
    }
}

impl Tool for EditFileTool {
    fn name(&self) -> &str {
        "edit_file"
    }

    fn execute(&self, args: serde_json::Value) -> Result<ToolResult, String> {
        let rel_path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing required parameter 'path'".to_string())?;

        let old_string = args
            .get("old_string")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let new_string = args
            .get("new_string")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing required parameter 'new_string'".to_string())?;

        let target_path = self.workspace_root.join(rel_path);

        if let PolicyDecision::Deny(reason) =
            self.policy.check_write(&self.workspace_root, &target_path)
        {
            return Ok(ToolResult {
                success: false,
                output: format!("Permission denied: {}", reason),
            });
        }

        if !target_path.exists() || old_string.trim().is_empty() {
            let write_tool = WriteFileTool::new(self.workspace_root.clone());
            return write_tool.execute(serde_json::json!({
                "path": rel_path,
                "content": new_string
            }));
        }

        let content = fs::read_to_string(&target_path)
            .map_err(|e| format!("Failed to read file for editing '{}': {}", rel_path, e))?;

        // 4-Tier Match Strategy:
        // Tier 1: Exact string match
        // Tier 2: Line-ending normalized match (\r\n vs \n)
        // Tier 3: Trimmed indentation line match
        // Tier 4: Fuzzy candidate detection -> NEVER auto-apply. Return diagnostic error for model correction.

        let final_updated_content = if content.contains(old_string) {
            // Tier 1: Exact match
            content.replace(old_string, new_string)
        } else {
            let normalized_content = content.replace("\r\n", "\n");
            let normalized_old = old_string.replace("\r\n", "\n");
            let normalized_new = new_string.replace("\r\n", "\n");

            if normalized_content.contains(&normalized_old) {
                // Tier 2: Line-ending normalized match
                normalized_content.replace(&normalized_old, &normalized_new)
            } else {
                // Tier 3: Check line-by-line trimmed match
                let content_lines: Vec<&str> = normalized_content.lines().collect();
                let old_lines: Vec<&str> = normalized_old.lines().collect();

                let mut match_start_idx = None;
                if !old_lines.is_empty() && content_lines.len() >= old_lines.len() {
                    for i in 0..=(content_lines.len() - old_lines.len()) {
                        let mut matches = true;
                        for j in 0..old_lines.len() {
                            if content_lines[i + j].trim() != old_lines[j].trim() {
                                matches = false;
                                break;
                            }
                        }
                        if matches {
                            match_start_idx = Some(i);
                            break;
                        }
                    }
                }

                if let Some(start_idx) = match_start_idx {
                    // Tier 3 match: replace matching lines while preserving original leading indentation of first line if possible
                    let end_idx = start_idx + old_lines.len();
                    let new_lines: Vec<&str> = normalized_new.lines().collect();

                    let mut result_lines = Vec::new();
                    result_lines.extend_from_slice(&content_lines[..start_idx]);
                    result_lines.extend_from_slice(&new_lines);
                    result_lines.extend_from_slice(&content_lines[end_idx..]);
                    result_lines.join("\n")
                } else {
                    // Tier 4: Rejection & Diagnostic output generator (Never silently apply)
                    let lines: Vec<&str> = content.lines().collect();
                    let mut diag = format!(
                        "EDIT_FAILED: Could not find matching target text in '{}' (Tier 1-3 match strategies failed).\n\n",
                        rel_path
                    );
                    diag.push_str("=== REQUESTED OLD_STRING ===\n");
                    let old_lines_preview: Vec<&str> = old_string.lines().take(10).collect();
                    diag.push_str(&old_lines_preview.join("\n"));
                    if old_string.lines().count() > 10 {
                        diag.push_str("\n... [truncated]");
                    }
                    diag.push_str("\n============================\n\n");

                    let trimmed_old = old_string.trim();
                    if !trimmed_old.is_empty() && content.contains(trimmed_old) {
                        diag.push_str("DIAGNOSTIC NOTE: Target text exists in file, but exact line indentation or whitespace differed.\n");
                    }

                    if lines.len() <= 40 {
                        diag.push_str("CURRENT FILE CONTENTS (Exact Line Numbers):\n");
                        for (idx, line) in lines.iter().enumerate() {
                            diag.push_str(&format!("{:4} | {}\n", idx + 1, line));
                        }
                    } else {
                        let first_line = old_string
                            .lines()
                            .find(|l| !l.trim().is_empty())
                            .unwrap_or("");
                        let matching_indices: Vec<usize> = lines
                            .iter()
                            .enumerate()
                            .filter(|(_, l)| {
                                !first_line.is_empty() && l.contains(first_line.trim())
                            })
                            .map(|(idx, _)| idx)
                            .collect();

                        if !matching_indices.is_empty() {
                            diag.push_str("MATCHING/SIMILAR LINE CONTEXT:\n");
                            for &match_idx in matching_indices.iter().take(3) {
                                let start = match_idx.saturating_sub(3);
                                let end = (match_idx + 4).min(lines.len());
                                diag.push_str(&format!("--- Near Line {} ---\n", match_idx + 1));
                                for (i, line) in lines.iter().enumerate().take(end).skip(start) {
                                    diag.push_str(&format!("{:4} | {}\n", i + 1, line));
                                }
                            }
                        } else {
                            diag.push_str("FILE SNIPPET (First 30 Lines):\n");
                            for (idx, line) in lines.iter().take(30).enumerate() {
                                diag.push_str(&format!("{:4} | {}\n", idx + 1, line));
                            }
                        }
                    }

                    diag.push_str("\nRECOVERY INSTRUCTION: Call 'read_file' on this path to get current exact line numbers and text, then retry 'edit_file' with exact line content.");

                    return Ok(ToolResult {
                        success: false,
                        output: diag,
                    });
                }
            }
        };

        fs::write(&target_path, final_updated_content)
            .map_err(|e| format!("Failed to save edited file '{}': {}", rel_path, e))?;

        Ok(ToolResult {
            success: true,
            output: format!("Successfully edited file {}", rel_path),
        })
    }
}

pub struct GrepTool {
    workspace_root: PathBuf,
    policy: PathPolicy,
}

impl GrepTool {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self {
            workspace_root,
            policy: PathPolicy,
        }
    }
}

impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn execute(&self, args: serde_json::Value) -> Result<ToolResult, String> {
        let pattern = args
            .get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing required parameter 'pattern'".to_string())?;

        let mut matches = Vec::new();
        let mut total_matches = 0;
        let max_matches = 100;

        let walker = WalkDir::new(&self.workspace_root)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                name != ".git" && name != "target" && name != "node_modules"
            });

        for entry in walker.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_file() {
                if let PolicyDecision::Deny(_) = self.policy.check_read(&self.workspace_root, path)
                {
                    continue;
                }

                if let Ok(content) = fs::read_to_string(path) {
                    let rel = path
                        .strip_prefix(&self.workspace_root)
                        .unwrap_or(path)
                        .to_string_lossy();

                    for (idx, line) in content.lines().enumerate() {
                        if line.contains(pattern) {
                            total_matches += 1;
                            if matches.len() < max_matches {
                                matches.push(format!("{}:{}: {}", rel, idx + 1, line));
                            }
                        }
                    }
                }
            }
        }

        let mut output = matches.join("\n");
        if total_matches > max_matches {
            output.push_str(&format!(
                "\n... [Truncated: showing {} of {} total matches]",
                max_matches, total_matches
            ));
        }

        Ok(ToolResult {
            success: true,
            output,
        })
    }
}

pub struct ListDirectoryTool {
    workspace_root: PathBuf,
    policy: PathPolicy,
}

impl ListDirectoryTool {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self {
            workspace_root,
            policy: PathPolicy,
        }
    }
}

impl Tool for ListDirectoryTool {
    fn name(&self) -> &str {
        "list_directory"
    }

    fn execute(&self, args: serde_json::Value) -> Result<ToolResult, String> {
        let rel_path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");

        let target_path = self.workspace_root.join(rel_path);

        if let PolicyDecision::Deny(reason) =
            self.policy.check_read(&self.workspace_root, &target_path)
        {
            return Ok(ToolResult {
                success: false,
                output: format!("Permission denied: {}", reason),
            });
        }

        if target_path.is_file() {
            return Ok(ToolResult {
                success: false,
                output: format!(
                    "Target '{}' is a file, not a directory. Use 'read_file' to view file contents.",
                    rel_path
                ),
            });
        }

        let entries = match fs::read_dir(&target_path) {
            Ok(rd) => rd,
            Err(e) => {
                return Ok(ToolResult {
                    success: false,
                    output: format!("Failed to list directory '{}': {}", rel_path, e),
                });
            }
        };

        let mut lines = Vec::new();
        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().to_string();
            let file_type = if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                "[DIR]"
            } else {
                "[FILE]"
            };
            lines.push(format!("{} {}", file_type, file_name));
        }

        lines.sort();

        Ok(ToolResult {
            success: true,
            output: lines.join("\n"),
        })
    }
}

pub struct FindSymbolTool {
    workspace_root: PathBuf,
}

impl FindSymbolTool {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self { workspace_root }
    }
}

impl Tool for FindSymbolTool {
    fn name(&self) -> &str {
        "find_symbol"
    }

    fn execute(&self, args: serde_json::Value) -> Result<ToolResult, String> {
        let name = args
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing required parameter 'name'".to_string())?;

        // 1. Try CodeGraph AST Symbol Query first
        if let Ok(mut graph) = chilli_repo_intel::CodeGraph::open_in_memory() {
            if graph.index_workspace(&self.workspace_root).is_ok() {
                if let Ok(nodes) = graph.lookup_nodes(name) {
                    if !nodes.is_empty() {
                        let matches: Vec<String> = nodes
                            .into_iter()
                            .take(20)
                            .map(|n| {
                                format!(
                                    "{}:{}:{}: {} ({})",
                                    n.file_path, n.start_line, n.end_line, n.qualified_name, n.kind
                                )
                            })
                            .collect();

                        return Ok(ToolResult {
                            success: true,
                            output: matches.join("\n"),
                        });
                    }
                }
            }
        }

        // 2. Fallback: Disk Walk & Substring Search
        let mut matches = Vec::new();
        let target_def = format!("def {}", name);
        let target_class = format!("class {}", name);

        let walker = WalkDir::new(&self.workspace_root)
            .into_iter()
            .filter_entry(|e| {
                let n = e.file_name().to_string_lossy();
                n != ".git" && n != "target" && n != "node_modules"
            });

        for entry in walker.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_file() && path.extension().map_or(false, |ext| ext == "py" || ext == "rs") {
                if let Ok(content) = fs::read_to_string(path) {
                    let rel = path
                        .strip_prefix(&self.workspace_root)
                        .unwrap_or(path)
                        .to_string_lossy()
                        .replace('\\', "/");
                    for (idx, line) in content.lines().enumerate() {
                        if line.contains(&target_def) || line.contains(&target_class) {
                            matches.push(format!("{}:{}: {}", rel, idx + 1, line.trim()));
                            if matches.len() >= 20 {
                                break;
                            }
                        }
                    }
                }
            }
            if matches.len() >= 20 {
                break;
            }
        }

        Ok(ToolResult {
            success: true,
            output: if matches.is_empty() {
                format!("Symbol '{}' not found in workspace.", name)
            } else {
                matches.join("\n")
            },
        })
    }
}

pub struct FindReferencesTool {
    workspace_root: PathBuf,
}

impl FindReferencesTool {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self { workspace_root }
    }
}

impl Tool for FindReferencesTool {
    fn name(&self) -> &str {
        "find_references"
    }

    fn execute(&self, args: serde_json::Value) -> Result<ToolResult, String> {
        let symbol = args
            .get("symbol")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing required parameter 'symbol'".to_string())?;

        // 1. Try CodeGraph AST Reference Edge Query
        if let Ok(mut graph) = chilli_repo_intel::CodeGraph::open_in_memory() {
            if graph.index_workspace(&self.workspace_root).is_ok() {
                if let Ok(ref_edges) = graph.references(symbol) {
                    if !ref_edges.is_empty() {
                        let matches: Vec<String> = ref_edges
                            .into_iter()
                            .take(20)
                            .map(|e| {
                                format!(
                                    "{}:{}: [{}] {} -> {}",
                                    e.edge.file_path,
                                    e.edge.line_number,
                                    e.edge.edge_type,
                                    e.source_node.qualified_name,
                                    e.target_node.qualified_name
                                )
                            })
                            .collect();

                        return Ok(ToolResult {
                            success: true,
                            output: matches.join("\n"),
                        });
                    }
                }
            }
        }

        // 2. Fallback: Grep Substring Search
        let grep = GrepTool::new(self.workspace_root.clone());
        grep.execute(serde_json::json!({ "pattern": symbol }))
    }
}

pub struct FindTestForFailureTool {
    workspace_root: PathBuf,
}

impl FindTestForFailureTool {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self { workspace_root }
    }
}

impl Tool for FindTestForFailureTool {
    fn name(&self) -> &str {
        "find_test_for_failure"
    }

    fn execute(&self, args: serde_json::Value) -> Result<ToolResult, String> {
        let traceback = args
            .get("traceback")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing required parameter 'traceback'".to_string())?;

        for line in traceback.lines() {
            if line.contains("File \"") && (line.contains("test") || line.contains("tests/")) {
                if let Some(start) = line.find("File \"") {
                    let rest = &line[start + 6..];
                    if let Some(end) = rest.find('"') {
                        let rel_path = rest[..end].replace('\\', "/");
                        let full_path = self.workspace_root.join(&rel_path);
                        if full_path.exists() {
                            if let Ok(content) = fs::read_to_string(&full_path) {
                                let lines: Vec<&str> = content.lines().collect();
                                let test_slice = lines[..lines.len().min(60)].join("\n");
                                return Ok(ToolResult {
                                    success: true,
                                    output: format!("--- FAILING TEST FILE: {} ---\n{}", rel_path, test_slice),
                                });
                            }
                        }
                    }
                }
            }
        }

        Ok(ToolResult {
            success: false,
            output: "No test file matching traceback path was found in workspace.".to_string(),
        })
    }
}

fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}

pub type MoveRefChecker = std::sync::Arc<dyn Fn(&str) -> usize + Send + Sync>;

pub struct MoveFileTool {
    workspace_root: PathBuf,
    policy: PathPolicy,
    ref_checker: Option<MoveRefChecker>,
}

impl MoveFileTool {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self {
            workspace_root,
            policy: PathPolicy,
            ref_checker: None,
        }
    }

    pub fn with_ref_checker(mut self, checker: MoveRefChecker) -> Self {
        self.ref_checker = Some(checker);
        self
    }
}

impl Tool for MoveFileTool {
    fn name(&self) -> &str {
        "move_file"
    }

    fn execute(&self, args: serde_json::Value) -> Result<ToolResult, String> {
        let from_rel = args
            .get("from")
            .or_else(|| args.get("src"))
            .or_else(|| args.get("source"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing required parameter 'from'".to_string())?;

        let to_rel = args
            .get("to")
            .or_else(|| args.get("dst"))
            .or_else(|| args.get("target"))
            .or_else(|| args.get("destination"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing required parameter 'to'".to_string())?;

        let from_path = self.workspace_root.join(from_rel);
        let to_path = self.workspace_root.join(to_rel);

        if let PolicyDecision::Deny(reason) =
            self.policy.check_read(&self.workspace_root, &from_path)
        {
            return Ok(ToolResult {
                success: false,
                output: format!("Permission denied reading source: {}", reason),
            });
        }

        if let PolicyDecision::Deny(reason) =
            self.policy.check_write(&self.workspace_root, &to_path)
        {
            return Ok(ToolResult {
                success: false,
                output: format!("Permission denied writing target: {}", reason),
            });
        }

        let copy_first = args
            .get("copy_first")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let force = args.get("force").and_then(|v| v.as_bool()).unwrap_or(false);

        if !copy_first && !force {
            if let Some(checker) = &self.ref_checker {
                let ref_count = checker(from_rel);
                if ref_count > 0 {
                    return Ok(ToolResult {
                        success: false,
                        output: format!(
                            "[DESTRUCTIVE MOVE GUARD] Cannot relocate '{}' directly because it has {} active incoming symbol reference(s). Use copy-first pattern ('copy_first': true) to preserve source recoverability during refactoring.",
                            from_rel, ref_count
                        ),
                    });
                }
            }
        }

        if copy_first {
            if let Some(parent) = to_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if from_path.is_dir() {
                if let Err(e) = copy_dir_all(&from_path, &to_path) {
                    return Ok(ToolResult {
                        success: false,
                        output: format!(
                            "Failed copy-first directory from '{}' to '{}': {}",
                            from_rel, to_rel, e
                        ),
                    });
                }
            } else {
                if let Err(e) = fs::copy(&from_path, &to_path) {
                    return Ok(ToolResult {
                        success: false,
                        output: format!(
                            "Failed copy-first file from '{}' to '{}': {}",
                            from_rel, to_rel, e
                        ),
                    });
                }
            }
            return Ok(ToolResult {
                success: true,
                output: format!(
                    "Copied '{}' to '{}' (copy-first mode enabled; source preserved).",
                    from_rel, to_rel
                ),
            });
        }

        if let Some(parent) = to_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        if fs::rename(&from_path, &to_path).is_err() {
            if from_path.is_dir() {
                if let Err(e) = copy_dir_all(&from_path, &to_path) {
                    return Ok(ToolResult {
                        success: false,
                        output: format!(
                            "Failed to move directory '{}' to '{}': {}",
                            from_rel, to_rel, e
                        ),
                    });
                }
                let _ = fs::remove_dir_all(&from_path);
            } else {
                if let Err(e) = fs::copy(&from_path, &to_path) {
                    return Ok(ToolResult {
                        success: false,
                        output: format!(
                            "Failed to move file '{}' to '{}': {}",
                            from_rel, to_rel, e
                        ),
                    });
                }
                let _ = fs::remove_file(&from_path);
            }
        }

        Ok(ToolResult {
            success: true,
            output: format!("Successfully moved '{}' to '{}'", from_rel, to_rel),
        })
    }
}

pub struct CopyFileTool {
    workspace_root: PathBuf,
    policy: PathPolicy,
}

impl CopyFileTool {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self {
            workspace_root,
            policy: PathPolicy,
        }
    }
}

impl Tool for CopyFileTool {
    fn name(&self) -> &str {
        "copy_file"
    }

    fn execute(&self, args: serde_json::Value) -> Result<ToolResult, String> {
        let from_rel = args
            .get("from")
            .or_else(|| args.get("src"))
            .or_else(|| args.get("source"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing required parameter 'from'".to_string())?;

        let to_rel = args
            .get("to")
            .or_else(|| args.get("dst"))
            .or_else(|| args.get("target"))
            .or_else(|| args.get("destination"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing required parameter 'to'".to_string())?;

        let from_path = self.workspace_root.join(from_rel);
        let to_path = self.workspace_root.join(to_rel);

        if let PolicyDecision::Deny(reason) =
            self.policy.check_read(&self.workspace_root, &from_path)
        {
            return Ok(ToolResult {
                success: false,
                output: format!("Permission denied reading source: {}", reason),
            });
        }

        if let PolicyDecision::Deny(reason) =
            self.policy.check_write(&self.workspace_root, &to_path)
        {
            return Ok(ToolResult {
                success: false,
                output: format!("Permission denied writing target: {}", reason),
            });
        }

        if let Some(parent) = to_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        if from_path.is_dir() {
            if let Err(e) = copy_dir_all(&from_path, &to_path) {
                return Ok(ToolResult {
                    success: false,
                    output: format!(
                        "Failed to copy directory '{}' to '{}': {}",
                        from_rel, to_rel, e
                    ),
                });
            }
        } else {
            if let Err(e) = fs::copy(&from_path, &to_path) {
                return Ok(ToolResult {
                    success: false,
                    output: format!("Failed to copy file '{}' to '{}': {}", from_rel, to_rel, e),
                });
            }
        }

        Ok(ToolResult {
            success: true,
            output: format!("Successfully copied '{}' to '{}'", from_rel, to_rel),
        })
    }
}

pub struct CreateDirectoryTool {
    workspace_root: PathBuf,
    policy: PathPolicy,
}

impl CreateDirectoryTool {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self {
            workspace_root,
            policy: PathPolicy,
        }
    }
}

impl Tool for CreateDirectoryTool {
    fn name(&self) -> &str {
        "create_directory"
    }

    fn execute(&self, args: serde_json::Value) -> Result<ToolResult, String> {
        let rel_path = args
            .get("path")
            .or_else(|| args.get("dir"))
            .or_else(|| args.get("directory"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing required parameter 'path'".to_string())?;

        let target_path = self.workspace_root.join(rel_path);

        if let PolicyDecision::Deny(reason) =
            self.policy.check_write(&self.workspace_root, &target_path)
        {
            return Ok(ToolResult {
                success: false,
                output: format!("Permission denied: {}", reason),
            });
        }

        // File/Directory Guard Check (P0): Catch path type confusion
        if target_path.exists() && target_path.is_file() {
            return Ok(ToolResult {
                success: false,
                output: format!(
                    "PATH_TYPE_ERROR: Target path '{}' exists as a file, not a directory. Use 'write_file' or 'edit_file' to modify existing files.",
                    rel_path
                ),
            });
        }

        // File extension heuristic: Common source/data file extensions indicate file target intent
        if let Some(ext) = std::path::Path::new(rel_path)
            .extension()
            .and_then(|e| e.to_str())
        {
            let ext_lower = ext.to_lowercase();
            const STANDARD_FILE_EXTENSIONS: &[&str] = &[
                "rs",
                "py",
                "js",
                "ts",
                "jsx",
                "tsx",
                "toml",
                "json",
                "md",
                "txt",
                "c",
                "cpp",
                "h",
                "hpp",
                "go",
                "java",
                "sh",
                "cmd",
                "bat",
                "css",
                "html",
                "xml",
                "yaml",
                "yml",
                "sql",
                "rb",
                "php",
                "cs",
                "kt",
                "swift",
                "lock",
                "csv",
                "log",
                "ini",
                "env",
                "gitignore",
            ];
            if STANDARD_FILE_EXTENSIONS.contains(&ext_lower.as_str()) {
                return Ok(ToolResult {
                    success: false,
                    output: format!(
                        "PATH_TYPE_ERROR: Target path '{}' has file extension '.{}'. Use 'write_file' to create new files instead of 'create_directory'.",
                        rel_path, ext
                    ),
                });
            }
        }

        if let Err(e) = fs::create_dir_all(&target_path) {
            return Ok(ToolResult {
                success: false,
                output: format!("Failed to create directory '{}': {}", rel_path, e),
            });
        }

        Ok(ToolResult {
            success: true,
            output: format!("Successfully created directory '{}'", rel_path),
        })
    }
}

pub struct RemoveFileTool {
    workspace_root: PathBuf,
    policy: PathPolicy,
}

impl RemoveFileTool {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self {
            workspace_root,
            policy: PathPolicy,
        }
    }
}

impl Tool for RemoveFileTool {
    fn name(&self) -> &str {
        "remove_file"
    }

    fn execute(&self, args: serde_json::Value) -> Result<ToolResult, String> {
        let rel_path = args
            .get("path")
            .or_else(|| args.get("target"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing required parameter 'path'".to_string())?;

        let recursive = args
            .get("recursive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let target_path = self.workspace_root.join(rel_path);

        if let PolicyDecision::Deny(reason) =
            self.policy.check_write(&self.workspace_root, &target_path)
        {
            return Ok(ToolResult {
                success: false,
                output: format!("Permission denied: {}", reason),
            });
        }

        if !target_path.exists() {
            return Ok(ToolResult {
                success: true,
                output: format!("Path '{}' does not exist", rel_path),
            });
        }

        if target_path.is_dir() {
            if recursive {
                if let Err(e) = fs::remove_dir_all(&target_path) {
                    return Ok(ToolResult {
                        success: false,
                        output: format!(
                            "Failed to recursively remove directory '{}': {}",
                            rel_path, e
                        ),
                    });
                }
            } else {
                if let Err(e) = fs::remove_dir(&target_path) {
                    return Ok(ToolResult {
                        success: false,
                        output: format!(
                            "Failed to remove directory '{}' (use recursive=true if non-empty): {}",
                            rel_path, e
                        ),
                    });
                }
            }
        } else {
            if let Err(e) = fs::remove_file(&target_path) {
                return Ok(ToolResult {
                    success: false,
                    output: format!("Failed to remove file '{}': {}", rel_path, e),
                });
            }
        }

        Ok(ToolResult {
            success: true,
            output: format!("Successfully removed '{}'", rel_path),
        })
    }
}

pub struct ExecTool {
    workspace_root: PathBuf,
    policy: PathPolicy,
}

impl ExecTool {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self {
            workspace_root,
            policy: PathPolicy,
        }
    }

    fn try_intercept_posix_cmd(
        &self,
        binary: &str,
        cmd_args: &[String],
    ) -> Option<Result<ToolResult, String>> {
        let b = binary.to_lowercase();
        match b.as_str() {
            "cd" => {
                let target_dir = cmd_args.first().cloned().unwrap_or_else(|| ".".to_string());
                Some(Ok(ToolResult {
                    success: true,
                    output: format!(
                        "Context directory is '{}'. Use 'cwd' argument in exec tool or list_directory tool.",
                        target_dir
                    ),
                }))
            }
            "mv" | "move" => {
                let positional: Vec<&String> =
                    cmd_args.iter().filter(|a| !a.starts_with('-')).collect();
                if positional.len() >= 2 {
                    let move_tool = MoveFileTool::new(self.workspace_root.clone());
                    Some(move_tool.execute(serde_json::json!({
                        "from": positional[0],
                        "to": positional[1]
                    })))
                } else {
                    None
                }
            }
            "cp" | "copy" => {
                let positional: Vec<&String> =
                    cmd_args.iter().filter(|a| !a.starts_with('-')).collect();
                if positional.len() >= 2 {
                    let copy_tool = CopyFileTool::new(self.workspace_root.clone());
                    Some(copy_tool.execute(serde_json::json!({
                        "from": positional[0],
                        "to": positional[1]
                    })))
                } else {
                    None
                }
            }
            "mkdir" => {
                let positional: Vec<&String> =
                    cmd_args.iter().filter(|a| !a.starts_with('-')).collect();
                if let Some(target) = positional.first() {
                    let mkdir_tool = CreateDirectoryTool::new(self.workspace_root.clone());
                    Some(mkdir_tool.execute(serde_json::json!({
                        "path": target
                    })))
                } else {
                    None
                }
            }
            "rm" | "del" => {
                let recursive = cmd_args.iter().any(|a| a.contains('r') || a.contains('f'));
                let positional: Vec<&String> =
                    cmd_args.iter().filter(|a| !a.starts_with('-')).collect();
                if let Some(target) = positional.first() {
                    let rm_tool = RemoveFileTool::new(self.workspace_root.clone());
                    Some(rm_tool.execute(serde_json::json!({
                        "path": target,
                        "recursive": recursive
                    })))
                } else {
                    None
                }
            }
            "grep" => {
                let positional: Vec<&String> =
                    cmd_args.iter().filter(|a| !a.starts_with('-')).collect();
                if let Some(pattern) = positional.first() {
                    let grep_tool = GrepTool::new(self.workspace_root.clone());
                    Some(grep_tool.execute(serde_json::json!({
                        "pattern": pattern
                    })))
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

impl Tool for ExecTool {
    fn name(&self) -> &str {
        "exec"
    }

    fn execute(&self, args: serde_json::Value) -> Result<ToolResult, String> {
        let raw_command = args
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing required parameter 'command'".to_string())?;

        let mut cmd_args: Vec<String> = args
            .get("args")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let mut binary = raw_command.trim().to_string();
        if binary.contains(' ') {
            let parts: Vec<String> = binary.split_whitespace().map(|s| s.to_string()).collect();
            if !parts.is_empty() {
                binary = parts[0].clone();
                let mut extra_args: Vec<String> = parts[1..].to_vec();
                extra_args.extend(cmd_args);
                cmd_args = extra_args;
            }
        }

        let rel_cwd = args.get("cwd").and_then(|v| v.as_str()).unwrap_or(".");

        let timeout_ms = args
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(120_000);

        let target_cwd = self.workspace_root.join(rel_cwd);

        let full_cmd_str = if cmd_args.is_empty() {
            binary.clone()
        } else {
            format!("{} {}", binary, cmd_args.join(" "))
        };

        if let PolicyDecision::Deny(reason) =
            self.policy
                .check_command(&self.workspace_root, &target_cwd, &full_cmd_str)
        {
            return Ok(ToolResult {
                success: false,
                output: format!("Permission denied: {}", reason),
            });
        }

        // Windows Shell Diagnostic Guard (P1): Catch single-quote syntax errors on Windows cmd.exe
        if cfg!(windows) {
            let has_single_quotes = full_cmd_str.contains('\'');
            let bin_lower = binary.to_lowercase();
            let is_script_exec = full_cmd_str.contains(" -c ")
                || full_cmd_str.contains(" -e ")
                || full_cmd_str.contains(" -Command ")
                || bin_lower == "python"
                || bin_lower == "python3"
                || bin_lower == "node"
                || bin_lower == "ruby"
                || bin_lower == "perl"
                || bin_lower == "bash"
                || bin_lower == "sh";

            if has_single_quotes && is_script_exec {
                return Ok(ToolResult {
                    success: false,
                    output: format!(
                        "COMMAND_REJECTED: Windows cmd.exe shell does not support single quotes (') for inline scripts or string arguments in command '{}'. Use double quotes escaped as \\\"...\\\" or write script to a file using 'write_file' and execute the script file.",
                        full_cmd_str
                    ),
                });
            }
        }

        if let Some(intercepted_res) = self.try_intercept_posix_cmd(&binary, &cmd_args) {
            return intercepted_res;
        }

        let caps = SandboxCapabilities::detect();
        let _is_isolated = caps.is_isolated;

        let mut cmd = std::process::Command::new(&binary);
        cmd.args(&cmd_args);
        cmd.current_dir(&target_cwd);

        // Sanitize sensitive host environment variables from child process
        const SENSITIVE_ENV_VARS: &[&str] = &[
            "OPENAI_API_KEY",
            "ANTHROPIC_API_KEY",
            "AWS_SECRET_ACCESS_KEY",
            "AWS_ACCESS_KEY_ID",
            "AWS_SESSION_TOKEN",
            "SSH_AUTH_SOCK",
            "GITHUB_TOKEN",
            "GH_TOKEN",
            "AZURE_CLIENT_SECRET",
            "DATABASE_URL",
            "PGPASSWORD",
            "MYSQL_PWD",
        ];
        for var in SENSITIVE_ENV_VARS {
            cmd.env_remove(var);
        }

        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    if let Some(fallback_res) = self.try_intercept_posix_cmd(&binary, &cmd_args) {
                        return fallback_res;
                    }
                }
                return Ok(ToolResult {
                    success: false,
                    output: format!("Failed to spawn process '{}': {}", binary, e),
                });
            }
        };

        let start = Instant::now();
        let timeout_duration = Duration::from_millis(timeout_ms);
        let poll_interval = Duration::from_millis(15);

        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let mut stdout_buf = Vec::new();
                    let mut stderr_buf = Vec::new();
                    if let Some(mut out) = child.stdout.take() {
                        let _ = std::io::Read::read_to_end(&mut out, &mut stdout_buf);
                    }
                    if let Some(mut err) = child.stderr.take() {
                        let _ = std::io::Read::read_to_end(&mut err, &mut stderr_buf);
                    }
                    let stdout = String::from_utf8_lossy(&stdout_buf);
                    let stderr = String::from_utf8_lossy(&stderr_buf);

                    let mut combined = String::new();
                    if !stdout.is_empty() {
                        combined.push_str(&stdout);
                    }
                    if !stderr.is_empty() {
                        if !combined.is_empty() {
                            combined.push('\n');
                        }
                        combined.push_str("stderr:\n");
                        combined.push_str(&stderr);
                    }

                    return Ok(ToolResult {
                        success: status.success(),
                        output: mask_credentials(&combined),
                    });
                }
                Ok(None) => {
                    if start.elapsed() >= timeout_duration {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Ok(ToolResult {
                            success: false,
                            output: format!(
                                "Process execution timed out after {}ms (process killed)",
                                timeout_ms
                            ),
                        });
                    }
                    std::thread::sleep(poll_interval);
                }
                Err(e) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Ok(ToolResult {
                        success: false,
                        output: format!("Error checking process status: {}", e),
                    });
                }
            }
        }
    }
}

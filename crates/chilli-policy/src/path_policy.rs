use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    Allow,
    Ask(String),
    Deny(String),
}

#[derive(Debug, Clone, Default)]
pub struct PathPolicy;

pub fn strip_unc_prefix(path: &Path) -> PathBuf {
    let mut s = path.to_string_lossy().to_string();
    while let Some(stripped) = s.strip_prefix(r"\\?\") {
        s = stripped.to_string();
    }
    PathBuf::from(s)
}

const SENSITIVE_PATTERNS: &[&str] = &[
    ".ssh",
    ".aws",
    ".gcp",
    ".azure",
    ".kdbx",
    ".env",
    ".git/config",
    ".git\\config",
    "id_rsa",
    "id_ed25519",
    "credentials.db",
    "secret.key",
    "passwd",
    "shadow",
    "etc/hosts",
    "etc\\hosts",
    "chrome/user data",
    "chrome\\user data",
    "physicaldrive",
    "rm -rf",
    "rmdir /s",
    "git push --force",
    "git push -f",
];

pub fn contains_sensitive_pattern(target_str: &str) -> Option<&'static str> {
    let normalized = target_str.to_lowercase().replace('\\', "/");
    for &pattern in SENSITIVE_PATTERNS {
        let pattern_normalized = pattern.to_lowercase().replace('\\', "/");
        if normalized.contains(&pattern_normalized) {
            return Some(pattern);
        }
    }
    None
}

impl PathPolicy {
    pub fn check_read(&self, workspace_root: &Path, target: &Path) -> PolicyDecision {
        self.evaluate(workspace_root, target)
    }

    pub fn check_write(&self, workspace_root: &Path, target: &Path) -> PolicyDecision {
        self.evaluate(workspace_root, target)
    }

    pub fn check_command(
        &self,
        workspace_root: &Path,
        cwd: &Path,
        command: &str,
    ) -> PolicyDecision {
        let resolved_cwd = if cwd.is_relative() {
            workspace_root.join(cwd)
        } else {
            cwd.to_path_buf()
        };

        let canonical_root = strip_unc_prefix(&match workspace_root.canonicalize() {
            Ok(p) => p,
            Err(_) => normalize_path(workspace_root),
        });

        let canonical_cwd =
            strip_unc_prefix(&match self.canonicalize_with_fallback(&resolved_cwd) {
                Ok(p) => p,
                Err(_) => {
                    return PolicyDecision::Deny("Failed to resolve working directory".to_string())
                }
            });

        if !is_contained(&canonical_root, &canonical_cwd) {
            return PolicyDecision::Deny(
                "Working directory outside workspace containment".to_string(),
            );
        }

        if let Some(pattern) = contains_sensitive_pattern(command) {
            return PolicyDecision::Deny(format!(
                "Command contains forbidden sensitive pattern '{}'",
                pattern
            ));
        }

        let token_decision = crate::command_policy::evaluate_command_tokens(command);
        if let PolicyDecision::Deny(_) = token_decision {
            return token_decision;
        }

        PolicyDecision::Allow
    }

    fn evaluate(&self, workspace_root: &Path, target: &Path) -> PolicyDecision {
        let resolved_target = if target.is_relative() {
            workspace_root.join(target)
        } else {
            target.to_path_buf()
        };

        let target_str = target.to_string_lossy();

        // 1. Sensitive file / directory check
        if contains_sensitive_pattern(&target_str).is_some() {
            return PolicyDecision::Deny("Sensitive path access forbidden".to_string());
        }
        for comp in target.components() {
            let comp_str = comp.as_os_str().to_string_lossy();
            if contains_sensitive_pattern(&comp_str).is_some() {
                return PolicyDecision::Deny("Sensitive path access forbidden".to_string());
            }
        }

        // 2. Canonicalization with UNC prefix stripping and parent fallback for non-existent paths
        let canonical_root = strip_unc_prefix(&match workspace_root.canonicalize() {
            Ok(p) => p,
            Err(_) => normalize_path(workspace_root),
        });

        let canonical_target =
            strip_unc_prefix(&match self.canonicalize_with_fallback(&resolved_target) {
                Ok(p) => p,
                Err(_) => return PolicyDecision::Deny("Failed to resolve target path".to_string()),
            });

        // 3. Workspace containment check
        if is_contained(&canonical_root, &canonical_target) {
            PolicyDecision::Allow
        } else {
            PolicyDecision::Deny("Path outside workspace containment".to_string())
        }
    }

    fn canonicalize_with_fallback(&self, path: &Path) -> std::io::Result<PathBuf> {
        let cleaned = strip_unc_prefix(path);
        if cleaned.exists() {
            cleaned.canonicalize().map(|p| strip_unc_prefix(&p))
        } else {
            let mut stack = Vec::new();
            let mut curr = cleaned;

            while !curr.exists() {
                if let Some(filename) = curr.file_name() {
                    stack.push(filename.to_os_string());
                    if let Some(parent) = curr.parent() {
                        curr = parent.to_path_buf();
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }

            let mut canonical = if curr.exists() {
                strip_unc_prefix(&curr.canonicalize()?)
            } else {
                curr
            };

            while let Some(segment) = stack.pop() {
                canonical.push(segment);
            }

            // Clean up any relative components (like .. or .)
            Ok(normalize_path(&canonical))
        }
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for comp in path.components() {
        match comp {
            std::path::Component::ParentDir => {
                components.pop();
            }
            std::path::Component::CurDir => {}
            c => components.push(c),
        }
    }
    components.iter().collect()
}

fn is_contained(root: &Path, target: &Path) -> bool {
    let root_norm = strip_unc_prefix(root);
    let target_norm = strip_unc_prefix(target);

    #[cfg(windows)]
    {
        let root_str = root_norm
            .to_string_lossy()
            .to_lowercase()
            .replace('/', "\\");
        let target_str = target_norm
            .to_string_lossy()
            .to_lowercase()
            .replace('/', "\\");
        if target_str == root_str {
            return true;
        }
        let root_with_sep = if root_str.ends_with('\\') {
            root_str
        } else {
            format!("{}\\", root_str)
        };
        target_str.starts_with(&root_with_sep)
    }
    #[cfg(not(windows))]
    {
        target_norm.starts_with(&root_norm)
    }
}

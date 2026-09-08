use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopAction {
    Continue,
    LockCall { reasoning_escalate: bool },
    TerminateTask,
}

impl LoopAction {
    pub fn is_terminate(&self) -> bool {
        matches!(self, LoopAction::TerminateTask)
    }

    pub fn is_lock(&self) -> bool {
        matches!(self, LoopAction::LockCall { .. })
    }
}

#[derive(Debug, Default)]
pub struct DuplicateLoopDetector {
    counts: HashMap<(String, u64), u32>,
    locked_hashes: HashSet<u64>,
}

impl DuplicateLoopDetector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn hash_args(args: &serde_json::Value) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        args.to_string().hash(&mut hasher);
        hasher.finish()
    }

    pub fn is_locked(&self, hash: u64) -> bool {
        self.locked_hashes.contains(&hash)
    }

    pub fn record_failure(
        &mut self,
        tool: &str,
        args: &serde_json::Value,
        _output: &str,
    ) -> LoopAction {
        let hash = Self::hash_args(args);
        let key = (tool.to_string(), hash);
        let count = self.counts.entry(key).or_insert(0);
        *count += 1;

        match *count {
            1 | 2 => LoopAction::Continue,
            3 => {
                self.locked_hashes.insert(hash);
                LoopAction::LockCall {
                    reasoning_escalate: true,
                }
            }
            _ => LoopAction::TerminateTask,
        }
    }

    pub fn record_success(&mut self, tool: &str, args: &serde_json::Value) {
        let hash = Self::hash_args(args);
        self.counts.remove(&(tool.to_string(), hash));
    }

    pub fn reset(&mut self) {
        self.counts.clear();
        self.locked_hashes.clear();
    }
}

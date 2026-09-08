use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlackboardArtifact {
    pub task_id: String,
    pub agent_id: String,
    pub path: PathBuf,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Blackboard {
    state: Arc<RwLock<BlackboardState>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BlackboardState {
    pub entries: HashMap<String, Value>,
    pub artifacts: Vec<BlackboardArtifact>,
    pub messages: Vec<String>,
}

impl Blackboard {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(BlackboardState::default())),
        }
    }

    pub fn set(&self, key: &str, value: Value) {
        if let Ok(mut lock) = self.state.write() {
            lock.entries.insert(key.to_string(), value);
        }
    }

    pub fn get(&self, key: &str) -> Option<Value> {
        self.state
            .read()
            .ok()
            .and_then(|lock| lock.entries.get(key).cloned())
    }

    pub fn add_artifact(&self, artifact: BlackboardArtifact) {
        if let Ok(mut lock) = self.state.write() {
            lock.artifacts.push(artifact);
        }
    }

    pub fn get_artifacts_for_task(&self, task_id: &str) -> Vec<BlackboardArtifact> {
        self.state
            .read()
            .ok()
            .map(|lock| {
                lock.artifacts
                    .iter()
                    .filter(|a| a.task_id == task_id)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn post_message(&self, message: impl Into<String>) {
        if let Ok(mut lock) = self.state.write() {
            lock.messages.push(message.into());
        }
    }

    pub fn get_messages(&self) -> Vec<String> {
        self.state
            .read()
            .ok()
            .map(|lock| lock.messages.clone())
            .unwrap_or_default()
    }

    pub fn clear_messages(&self) -> usize {
        if let Ok(mut lock) = self.state.write() {
            let count = lock.messages.len();
            lock.messages.clear();
            count
        } else {
            0
        }
    }

    pub fn count_entries(&self) -> usize {
        self.state
            .read()
            .ok()
            .map(|lock| lock.entries.len())
            .unwrap_or(0)
    }

    pub fn snapshot(&self) -> BlackboardState {
        self.state
            .read()
            .ok()
            .map(|lock| lock.clone())
            .unwrap_or_default()
    }
}

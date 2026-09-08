use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Failed to create directory '{path}': {source}")]
    DirectoryCreationFailed {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("Failed to write file '{path}': {source}")]
    FileWriteFailed {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("Failed to determine home directory")]
    HomeDirNotFound,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoragePaths {
    pub workspace_root: PathBuf,
    pub local_chilli_dir: PathBuf,
    pub journal_db_path: PathBuf,
    pub codegraph_db_path: PathBuf,
    pub local_memory_db_path: PathBuf,
    pub global_memory_db_path: PathBuf,
    pub task_store_db_path: PathBuf,
    pub cache_dir: PathBuf,
    pub global_config_path: PathBuf,
}

impl StoragePaths {
    pub fn resolve<P: AsRef<Path>>(workspace_root: P) -> Result<Self, StorageError> {
        let root = workspace_root.as_ref().to_path_buf();
        let local_chilli = root.join(".chilli");
        let cache_dir = local_chilli.join("cache");
        let journal_db_path = local_chilli.join("journal.db");
        let codegraph_db_path = local_chilli.join("codegraph.db");
        let local_memory_db_path = local_chilli.join("memory.db");
        let task_store_db_path = local_chilli.join("tasks.db");

        let home = dirs::home_dir().ok_or(StorageError::HomeDirNotFound)?;
        let global_chilli = home.join(".chilli");
        let global_config_path = global_chilli.join("config.toml");
        let global_memory_db_path = global_chilli.join("global_memory.db");

        fs::create_dir_all(&local_chilli).map_err(|e| StorageError::DirectoryCreationFailed {
            path: local_chilli.clone(),
            source: e,
        })?;

        fs::create_dir_all(&global_chilli).map_err(|e| StorageError::DirectoryCreationFailed {
            path: global_chilli.clone(),
            source: e,
        })?;

        fs::create_dir_all(&cache_dir).map_err(|e| StorageError::DirectoryCreationFailed {
            path: cache_dir.clone(),
            source: e,
        })?;

        let gitignore_path = local_chilli.join(".gitignore");
        if !gitignore_path.exists() {
            fs::write(&gitignore_path, "*\n").map_err(|e| StorageError::FileWriteFailed {
                path: gitignore_path,
                source: e,
            })?;
        }

        Ok(Self {
            workspace_root: root,
            local_chilli_dir: local_chilli,
            journal_db_path,
            codegraph_db_path,
            local_memory_db_path,
            global_memory_db_path,
            task_store_db_path,
            cache_dir,
            global_config_path,
        })
    }
}

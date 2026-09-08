use crate::store::{EdgeWithNodes, NodeRecord, SymbolRecord};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyntaxDiagnostic {
    pub file_path: String,
    pub line_number: usize,
    pub column: usize,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexSummary {
    pub files_indexed: usize,
    pub symbols_indexed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolImpactReport {
    pub symbol: String,
    pub file_path: String,
    pub kind: String,
    pub callers: Vec<String>,
    pub references: Vec<SymbolRecord>,
}

#[derive(Debug, Clone, Default)]
pub struct CodeGraph;

impl CodeGraph {
    pub fn open(_db_path: &Path) -> Result<Self, String> {
        Ok(Self)
    }

    pub fn open_in_memory() -> Result<Self, String> {
        Ok(Self)
    }

    pub fn index_file(&mut self, _path: &Path) -> Result<(), String> {
        Ok(())
    }

    pub fn index_workspace(&mut self, _workspace_root: &Path) -> Result<IndexSummary, String> {
        Ok(IndexSummary {
            files_indexed: 0,
            symbols_indexed: 0,
        })
    }

    pub fn lookup_symbol(&self, _name: &str) -> Result<Vec<SymbolRecord>, String> {
        Ok(Vec::new())
    }

    pub fn lookup_nodes(&self, _search_term: &str) -> Result<Vec<NodeRecord>, String> {
        Ok(Vec::new())
    }

    pub fn search_nodes_fts(&self, _query: &str, _limit: usize) -> Result<Vec<NodeRecord>, String> {
        Ok(Vec::new())
    }

    pub fn get_symbols_by_file(&self, _file_path: &str) -> Result<Vec<SymbolRecord>, String> {
        Ok(Vec::new())
    }

    pub fn callers(&self, _symbol_name: &str) -> Result<Vec<NodeRecord>, String> {
        Ok(Vec::new())
    }

    pub fn callees(&self, _symbol_name: &str) -> Result<Vec<NodeRecord>, String> {
        Ok(Vec::new())
    }

    pub fn references(&self, _symbol_name: &str) -> Result<Vec<EdgeWithNodes>, String> {
        Ok(Vec::new())
    }

    pub fn imports(&self, _module_name: &str) -> Result<Vec<EdgeWithNodes>, String> {
        Ok(Vec::new())
    }

    pub fn dependency_neighborhood(
        &self,
        _node_id: i64,
        _max_depth: usize,
    ) -> Result<Vec<NodeRecord>, String> {
        Ok(Vec::new())
    }

    pub fn connected_non_test_nodes(
        &self,
        _node_id: i64,
        _max_depth: usize,
    ) -> Result<Vec<NodeRecord>, String> {
        Ok(Vec::new())
    }

    pub fn connected_non_test_files(
        &self,
        _file_path: &str,
        _max_depth: usize,
    ) -> Result<Vec<String>, String> {
        Ok(Vec::new())
    }

    pub fn get_blast_radius(&self, symbol: &str, _depth: usize) -> Result<SymbolImpactReport, String> {
        Ok(SymbolImpactReport {
            symbol: symbol.to_string(),
            file_path: String::new(),
            kind: String::new(),
            callers: Vec::new(),
            references: Vec::new(),
        })
    }

    pub fn check_syntax(&self, path: &Path, content: &str) -> Vec<SyntaxDiagnostic> {
        let rel_path = path.to_string_lossy().replace('\\', "/");
        let mut diagnostics = Vec::new();

        let mut brace_depth: i64 = 0;
        let mut paren_depth: i64 = 0;

        for (i, line) in content.lines().enumerate() {
            let line_num = i + 1;
            let trimmed = line.trim();

            if trimmed.contains("= ;") || trimmed.contains("=;") {
                diagnostics.push(SyntaxDiagnostic {
                    file_path: rel_path.clone(),
                    line_number: line_num,
                    column: line.find('=').unwrap_or(0) + 1,
                    message: "Syntax error: missing expression after assignment operator '='".to_string(),
                });
            }

            for ch in line.chars() {
                match ch {
                    '{' => brace_depth += 1,
                    '}' => brace_depth -= 1,
                    '(' => paren_depth += 1,
                    ')' => paren_depth -= 1,
                    _ => {}
                }
            }
        }

        if brace_depth != 0 || paren_depth != 0 {
            if diagnostics.is_empty() {
                diagnostics.push(SyntaxDiagnostic {
                    file_path: rel_path,
                    line_number: 1,
                    column: 1,
                    message: "Syntax error: unclosed braces or parentheses".to_string(),
                });
            }
        }

        diagnostics
    }

    pub fn resolve_cross_module_dependencies(
        &self,
        _workspace_root: &Path,
        source_file: &str,
        _traceback: Option<&str>,
    ) -> Vec<String> {
        vec![source_file.to_string()]
    }
}

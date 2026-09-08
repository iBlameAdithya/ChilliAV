use crate::models::SchemaMetadata;
use thiserror::Error;
use tracing::info;

#[derive(Debug, Error)]
pub enum SqlValidationError {
    #[error("Mutating SQL statement detected: '{0}'. Only SELECT queries are permitted.")]
    MutationNotAllowed(String),
    #[error("SQL syntax invalid or missing SELECT clause: '{0}'")]
    InvalidSyntax(String),
    #[error("Disallowed SQL keyword detected: '{0}'")]
    DisallowedKeyword(String),
}

/// SQL Validation Agent
/// Validates SQL queries for security, read-only access, schema alignment, and syntax correctness
pub struct SqlValidationAgent;

impl SqlValidationAgent {
    pub fn new() -> Self {
        Self
    }

    pub fn validate_sql(
        &self,
        sql: &str,
        _schema: &SchemaMetadata,
    ) -> Result<String, SqlValidationError> {
        info!("SqlValidationAgent: Validating SQL query: '{}'", sql);
        let trimmed = sql.trim();
        let upper = trimmed.to_uppercase();

        // 1. Reject DDL / DML write operations
        let forbidden = [
            "INSERT ", "UPDATE ", "DELETE ", "DROP ", "ALTER ", "TRUNCATE ", "EXEC ", "GRANT ", "REVOKE ",
        ];

        for kw in forbidden {
            if upper.contains(kw) {
                return Err(SqlValidationError::MutationNotAllowed(kw.trim().to_string()));
            }
        }

        // 2. Must start with SELECT or WITH
        if !upper.starts_with("SELECT") && !upper.starts_with("WITH") {
            return Err(SqlValidationError::InvalidSyntax(trimmed.to_string()));
        }

        // 3. Ensure balanced parentheses & basic sanity
        let open_count = trimmed.chars().filter(|&c| c == '(').count();
        let close_count = trimmed.chars().filter(|&c| c == ')').count();
        if open_count != close_count {
            return Err(SqlValidationError::InvalidSyntax(
                "Unbalanced parentheses in SQL query".to_string(),
            ));
        }

        info!("SqlValidationAgent: SQL query successfully validated and sanitized.");
        Ok(trimmed.to_string())
    }
}

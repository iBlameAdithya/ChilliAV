use crate::db_mcp::EnterpriseDbManager;
use crate::models::{EnterpriseDomain, QueryIntent, SchemaMetadata, TableMetadata};
use tracing::info;

/// Schema Discovery Agent
/// Introspects database structures via MCP protocol and selects relevant enterprise tables & columns
pub struct SchemaDiscoveryAgent;

impl SchemaDiscoveryAgent {
    pub fn new() -> Self {
        Self
    }

    pub fn discover_relevant_schema(
        &self,
        intent: &QueryIntent,
        db_mgr: &EnterpriseDbManager,
    ) -> Result<SchemaMetadata, Box<dyn std::error::Error>> {
        info!(
            "SchemaDiscoveryAgent: Discovering schema via MCP for domain: {:?}",
            intent.domain
        );

        let full_schema = db_mgr.discover_schema()?;

        // Filter tables matching target entities or domain
        let filtered_tables: Vec<TableMetadata> = full_schema
            .tables
            .into_iter()
            .filter(|t| {
                if intent.domain == EnterpriseDomain::CrossDomain {
                    true // Include all tables for root cause cross-domain analysis
                } else {
                    intent.target_entities.contains(&t.table_name) || t.domain == intent.domain
                }
            })
            .collect();

        info!(
            "SchemaDiscoveryAgent: Discovered {} relevant tables ({})",
            filtered_tables.len(),
            filtered_tables
                .iter()
                .map(|t| t.table_name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );

        Ok(SchemaMetadata {
            tables: filtered_tables,
        })
    }
}

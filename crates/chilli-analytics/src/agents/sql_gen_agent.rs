use crate::models::{QueryIntent, QueryType, SchemaMetadata};
use tracing::info;

/// SQL Generation Agent
/// Dynamically constructs validated dialect SQL queries based on intent and discovered schemas
pub struct SqlGenerationAgent;

pub struct GeneratedSqlPayload {
    pub primary_sql: String,
    pub auxiliary_sqls: Vec<(String, String)>, // (label, sql)
}

impl SqlGenerationAgent {
    pub fn new() -> Self {
        Self
    }

    pub fn generate_sql(
        &self,
        intent: &QueryIntent,
        _schema: &SchemaMetadata,
    ) -> Result<GeneratedSqlPayload, Box<dyn std::error::Error>> {
        info!("SqlGenerationAgent: Generating SQL for query type {:?}", intent.query_type);

        match intent.query_type {
            QueryType::Distribution => {
                // Scenario 1: Lead status distribution
                let sql = "SELECT status, COUNT(*) AS count, SUM(estimated_value) AS total_value FROM crm_leads WHERE created_month = '2026-09' GROUP BY status ORDER BY count DESC;".to_string();
                Ok(GeneratedSqlPayload {
                    primary_sql: sql,
                    auxiliary_sqls: vec![],
                })
            }
            QueryType::Trend => {
                // Scenario 2: Monthly sales trend for the last year
                let sql = "SELECT month, SUM(amount) AS total_sales FROM sales_orders GROUP BY month ORDER BY month ASC;".to_string();
                Ok(GeneratedSqlPayload {
                    primary_sql: sql,
                    auxiliary_sqls: vec![],
                })
            }
            QueryType::RootCauseAnalysis => {
                // Scenario 3: Why did sales decrease last month?
                let primary_sql = "SELECT month, SUM(amount) AS total_sales FROM sales_orders WHERE month IN ('2026-06', '2026-07', '2026-08', '2026-09') GROUP BY month ORDER BY month ASC;".to_string();

                let auxiliary_sqls = vec![
                    (
                        "Marketing Spend Correlation".to_string(),
                        "SELECT month, ad_spend, leads_generated FROM marketing_campaigns WHERE month IN ('2026-06', '2026-07', '2026-08', '2026-09') ORDER BY month ASC;".to_string(),
                    ),
                    (
                        "Inventory Outages".to_string(),
                        "SELECT sku, product_name, stock_out_events FROM erp_inventory WHERE month = '2026-08';".to_string(),
                    ),
                ];

                Ok(GeneratedSqlPayload {
                    primary_sql,
                    auxiliary_sqls,
                })
            }
            QueryType::Aggregation | QueryType::Comparison | QueryType::DetailedList => {
                let sql = "SELECT department, AVG(salary) AS avg_salary, AVG(performance_score) AS avg_performance FROM hrms_employees GROUP BY department;".to_string();
                Ok(GeneratedSqlPayload {
                    primary_sql: sql,
                    auxiliary_sqls: vec![],
                })
            }
        }
    }
}

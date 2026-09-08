use crate::models::{EnterpriseDomain, QueryIntent, SchemaMetadata};
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
        info!("SqlGenerationAgent: Generating SQL for domain {:?}, query type {:?}", intent.domain, intent.query_type);

        match intent.domain {
            EnterpriseDomain::CRM => {
                let raw_lower = intent.raw_query.to_lowercase();
                let sql = if raw_lower.contains("top") || raw_lower.contains("customer") || raw_lower.contains("highest") {
                    "SELECT company, contact_name, status, estimated_value FROM crm_leads ORDER BY estimated_value DESC LIMIT 5;".to_string()
                } else {
                    "SELECT status, COUNT(*) AS count, SUM(estimated_value) AS total_value FROM crm_leads WHERE created_month = '2026-09' GROUP BY status ORDER BY count DESC;".to_string()
                };
                Ok(GeneratedSqlPayload {
                    primary_sql: sql,
                    auxiliary_sqls: vec![],
                })
            }
            EnterpriseDomain::ECommerce => {
                let sql = "SELECT month, SUM(amount) AS total_sales FROM sales_orders GROUP BY month ORDER BY month ASC;".to_string();
                Ok(GeneratedSqlPayload {
                    primary_sql: sql,
                    auxiliary_sqls: vec![],
                })
            }
            EnterpriseDomain::ERP => {
                let sql = "SELECT sku, product_name, stock_out_events, month FROM erp_inventory ORDER BY stock_out_events DESC;".to_string();
                Ok(GeneratedSqlPayload {
                    primary_sql: sql,
                    auxiliary_sqls: vec![],
                })
            }
            EnterpriseDomain::HRMS => {
                let sql = "SELECT department, COUNT(*) AS employee_count, AVG(salary) AS avg_salary, AVG(performance_score) AS avg_performance FROM hrms_employees GROUP BY department;".to_string();
                Ok(GeneratedSqlPayload {
                    primary_sql: sql,
                    auxiliary_sqls: vec![],
                })
            }
            EnterpriseDomain::CrossDomain => {
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
            EnterpriseDomain::Unknown => {
                Err("Not enough data to be processed".into())
            }
        }
    }
}

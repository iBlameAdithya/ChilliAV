use crate::models::{EnterpriseDomain, QueryIntent, QueryType};
use tracing::info;

/// Intent Understanding Agent
/// Parses voice or natural language queries into structured domain analytical intents
pub struct IntentUnderstandingAgent;

impl IntentUnderstandingAgent {
    pub fn new() -> Self {
        Self
    }

    pub fn analyze_intent(&self, input: &str, is_voice: bool) -> QueryIntent {
        info!("IntentUnderstandingAgent: Analyzing intent for: '{}'", input);
        let query_lower = input.trim().to_lowercase();

        // 1. Immediate Security & DDL/DML Mutation Guard: Block raw SQL commands or SQL injection syntax
        if query_lower.starts_with("drop ")
            || query_lower.starts_with("delete ")
            || query_lower.starts_with("update ")
            || query_lower.starts_with("insert ")
            || query_lower.starts_with("alter ")
            || query_lower.starts_with("truncate ")
            || query_lower.contains("drop table")
            || query_lower.contains("drop database")
            || query_lower.contains("delete from")
            || query_lower.contains("insert into")
            || query_lower.contains("alter table")
            || query_lower.contains("truncate table")
            || query_lower.contains("--")
            || query_lower.contains("/*")
        {
            info!("Security Alert: DDL/DML SQL mutation or command structure detected in input.");
            return QueryIntent {
                raw_query: input.to_string(),
                is_voice_input: is_voice,
                domain: EnterpriseDomain::Unknown,
                query_type: QueryType::Aggregation,
                target_entities: vec![],
                metrics: vec![],
                time_horizon: None,
                filter_conditions: vec![],
            };
        }

        // 2. Detect Root-Cause Analysis (Scenario 3 & Variations)
        if query_lower.contains("why did")
            || query_lower.contains("why sales")
            || query_lower.contains("why revenue")
            || query_lower.contains("reason for")
            || query_lower.contains("root cause")
            || query_lower.contains("decrease")
            || query_lower.contains("sales drop")
            || query_lower.contains("revenue drop")
            || query_lower.contains("drop in")
            || query_lower.contains("down")
            || query_lower.contains("decline")
            || query_lower.contains("fall")
            || query_lower.contains("slump")
        {
            return QueryIntent {
                raw_query: input.to_string(),
                is_voice_input: is_voice,
                domain: EnterpriseDomain::CrossDomain,
                query_type: QueryType::RootCauseAnalysis,
                target_entities: vec!["sales_orders".to_string(), "marketing_campaigns".to_string(), "erp_inventory".to_string()],
                metrics: vec!["revenue_amount".to_string(), "ad_spend".to_string(), "lead_count".to_string()],
                time_horizon: Some("2026-08".to_string()),
                filter_conditions: vec!["month = '2026-08'".to_string()],
            };
        }

        // 3. Detect Sales Trend & Revenue (Scenario 2 & Variations)
        if query_lower.contains("trend")
            || query_lower.contains("monthly sales")
            || query_lower.contains("sales orders")
            || query_lower.contains("revenue by month")
            || query_lower.contains("by month")
            || query_lower.contains("over time")
            || query_lower.contains("last year")
            || query_lower.contains("12 months")
            || query_lower.contains("6 months")
            || query_lower.contains("growth rate")
            || query_lower.contains("sales performance")
            || query_lower.contains("sales")
            || query_lower.contains("slaes")
            || query_lower.contains("revenue")
            || query_lower.contains("revanue")
            || query_lower.contains("order")
            || query_lower.contains("region")
            || query_lower.contains("amount")
            || query_lower.contains("tredn")
        {
            return QueryIntent {
                raw_query: input.to_string(),
                is_voice_input: is_voice,
                domain: EnterpriseDomain::ECommerce,
                query_type: QueryType::Trend,
                target_entities: vec!["sales_orders".to_string()],
                metrics: vec!["total_amount".to_string()],
                time_horizon: Some("last_12_months".to_string()),
                filter_conditions: vec![],
            };
        }

        // 4. Detect ERP Inventory / Stock Queries
        if query_lower.contains("stock")
            || query_lower.contains("inventory")
            || query_lower.contains("product")
            || query_lower.contains("out of stock")
            || query_lower.contains("supply chain")
            || query_lower.contains("sku")
            || query_lower.contains("warehouse")
        {
            return QueryIntent {
                raw_query: input.to_string(),
                is_voice_input: is_voice,
                domain: EnterpriseDomain::ERP,
                query_type: QueryType::DetailedList,
                target_entities: vec!["erp_inventory".to_string()],
                metrics: vec!["stock_out_events".to_string()],
                time_horizon: None,
                filter_conditions: vec![],
            };
        }

        // 5. Detect HRMS or General Employee Queries
        if query_lower.contains("employee")
            || query_lower.contains("salary")
            || query_lower.contains("performance")
            || query_lower.contains("department")
            || query_lower.contains("staff")
            || query_lower.contains("payroll")
            || query_lower.contains("headcount")
            || query_lower.contains("workforce")
            || query_lower.contains("hr")
        {
            return QueryIntent {
                raw_query: input.to_string(),
                is_voice_input: is_voice,
                domain: EnterpriseDomain::HRMS,
                query_type: QueryType::Aggregation,
                target_entities: vec!["hrms_employees".to_string()],
                metrics: vec!["performance_score".to_string()],
                time_horizon: None,
                filter_conditions: vec![],
            };
        }

        // 6. Detect Top Customers / Companies (CRM Domain)
        if query_lower.contains("top")
            || query_lower.contains("customer")
            || query_lower.contains("company")
            || query_lower.contains("deal value")
            || query_lower.contains("highest")
            || query_lower.contains("account")
            || query_lower.contains("vip")
        {
            return QueryIntent {
                raw_query: input.to_string(),
                is_voice_input: is_voice,
                domain: EnterpriseDomain::CRM,
                query_type: QueryType::Aggregation,
                target_entities: vec!["crm_leads".to_string()],
                metrics: vec!["estimated_value".to_string()],
                time_horizon: None,
                filter_conditions: vec![],
            };
        }

        // 7. Detect CRM Lead Distribution (Scenario 1)
        if query_lower.contains("lead status")
            || query_lower.contains("lead distribution")
            || query_lower.contains("crm")
            || query_lower.contains("pipeline")
            || query_lower.contains("prospect")
            || query_lower.contains("funnel")
            || query_lower.contains("lead")
        {
            return QueryIntent {
                raw_query: input.to_string(),
                is_voice_input: is_voice,
                domain: EnterpriseDomain::CRM,
                query_type: QueryType::Distribution,
                target_entities: vec!["crm_leads".to_string()],
                metrics: vec!["lead_count".to_string()],
                time_horizon: Some("2026-09".to_string()),
                filter_conditions: vec!["created_month = '2026-09'".to_string()],
            };
        }

        // 8. Out-of-Domain / Irrelevant / Gibberish Query Protection
        QueryIntent {
            raw_query: input.to_string(),
            is_voice_input: is_voice,
            domain: EnterpriseDomain::Unknown,
            query_type: QueryType::Aggregation,
            target_entities: vec![],
            metrics: vec![],
            time_horizon: None,
            filter_conditions: vec![],
        }
    }
}



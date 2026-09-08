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
        let query_lower = input.to_lowercase();

        // Detect Root-Cause Analysis (Scenario 3)
        if query_lower.contains("why did")
            || query_lower.contains("why sales")
            || query_lower.contains("reason for")
            || query_lower.contains("root cause")
            || query_lower.contains("decrease")
            || query_lower.contains("drop")
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

        // Detect Sales Trend (Scenario 2)
        if query_lower.contains("trend")
            || query_lower.contains("monthly sales")
            || query_lower.contains("over time")
            || query_lower.contains("last year")
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

        // Detect CRM Lead Distribution (Scenario 1)
        if query_lower.contains("lead status")
            || query_lower.contains("lead distribution")
            || query_lower.contains("crm")
            || query_lower.contains("pipeline")
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

        // Fallback HRMS or General Aggregation
        if query_lower.contains("employee") || query_lower.contains("salary") || query_lower.contains("performance") {
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

        // Default intent
        QueryIntent {
            raw_query: input.to_string(),
            is_voice_input: is_voice,
            domain: EnterpriseDomain::CRM,
            query_type: QueryType::Distribution,
            target_entities: vec!["crm_leads".to_string()],
            metrics: vec!["count".to_string()],
            time_horizon: Some("current_month".to_string()),
            filter_conditions: vec![],
        }
    }
}

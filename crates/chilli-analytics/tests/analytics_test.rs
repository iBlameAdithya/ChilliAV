use chilli_analytics::agents::{SqlValidationAgent, SqlValidationError, VoiceProcessingAgent};
use chilli_analytics::db_mcp::EnterpriseDbManager;
use chilli_analytics::models::{ChartType, EnterpriseDomain, QueryType};
use chilli_analytics::orchestrator::AnalyticsMultiAgentOrchestrator;

#[test]
fn test_voice_processing_agent() {
    let agent = VoiceProcessingAgent::new();
    let (processed, is_voice) = agent.process_input("um please show me last year sales trenduh", true);
    assert!(is_voice);
    assert!(processed.contains("Show"));
    assert!(processed.contains("sales trend"));
}

#[test]
fn test_sql_validation_agent_security() {
    let db_mgr = EnterpriseDbManager::new_in_memory().unwrap();
    let schema = db_mgr.discover_schema().unwrap();
    let val_agent = SqlValidationAgent::new();

    // Valid query
    let valid = val_agent.validate_sql("SELECT * FROM crm_leads", &schema);
    assert!(valid.is_ok());

    // Injection / Mutation query
    let mutation = val_agent.validate_sql("DROP TABLE crm_leads", &schema);
    assert!(mutation.is_err());
    if let Err(SqlValidationError::MutationNotAllowed(kw)) = mutation {
        assert_eq!(kw, "DROP");
    } else {
        panic!("Expected MutationNotAllowed error");
    }

    // Unbalanced parens
    let unbalanced = val_agent.validate_sql("SELECT COUNT( FROM crm_leads", &schema);
    assert!(unbalanced.is_err());
}

#[test]
fn test_scenario_1_lead_distribution() {
    let orchestrator = AnalyticsMultiAgentOrchestrator::new().unwrap();
    let result = orchestrator
        .execute_query("Show lead status distribution for this month.", false)
        .unwrap();

    assert_eq!(result.dashboard.domain, EnterpriseDomain::CRM);
    assert_eq!(result.dashboard.query_type, QueryType::Distribution);
    assert_eq!(result.dashboard.widgets.len(), 1);
    assert_eq!(result.dashboard.widgets[0].viz_type, ChartType::PieChart);
    assert!(result.dashboard.widgets[0].data.row_count > 0);
}

#[test]
fn test_scenario_2_sales_trend() {
    let orchestrator = AnalyticsMultiAgentOrchestrator::new().unwrap();
    let result = orchestrator
        .execute_query("Show monthly sales trend for the last year.", false)
        .unwrap();

    assert_eq!(result.dashboard.domain, EnterpriseDomain::ECommerce);
    assert_eq!(result.dashboard.query_type, QueryType::Trend);
    assert_eq!(result.dashboard.widgets[0].viz_type, ChartType::LineChart);
    assert_eq!(result.dashboard.widgets[0].data.row_count, 12);
}

#[test]
fn test_scenario_3_root_cause_analysis() {
    let orchestrator = AnalyticsMultiAgentOrchestrator::new().unwrap();
    let result = orchestrator
        .execute_query("Why did sales decrease last month?", false)
        .unwrap();

    assert_eq!(result.dashboard.domain, EnterpriseDomain::CrossDomain);
    assert_eq!(result.dashboard.query_type, QueryType::RootCauseAnalysis);
    assert!(result.dashboard.widgets.len() >= 2); // Primary + Auxiliary
    assert!(!result.root_cause.contributing_factors.is_empty());
    assert!(!result.root_cause.actionable_recommendations.is_empty());
}

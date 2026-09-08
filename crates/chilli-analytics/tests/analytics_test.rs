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

#[tokio::test]
async fn test_mcp_database_tool_execution() {
    let db_mgr = EnterpriseDbManager::new_in_memory().unwrap();
    let tools = db_mgr.mcp_list_tools();
    assert_eq!(tools.tools.len(), 2);

    let call_params = chilli_mcp::protocol::McpCallToolParams {
        name: "mcp_enterprise_execute_sql".to_string(),
        arguments: Some(serde_json::json!({
            "sql": "SELECT COUNT(*) FROM sales_orders"
        })),
    };

    let result = db_mgr.mcp_call_tool(call_params).unwrap();
    assert!(!result.is_error);
    assert_eq!(result.content.len(), 1);
}

#[test]
fn test_phase_3_schema_caching_and_read_only_guard() {
    let db_mgr = EnterpriseDbManager::new_in_memory().unwrap();
    assert!(db_mgr.provider_type().contains("SQLite"));

    // 1. Schema discovery cached
    let schema1 = db_mgr.discover_schema().unwrap();
    let schema2 = db_mgr.discover_schema().unwrap();
    assert_eq!(schema1.tables.len(), schema2.tables.len());

    // 2. Read-only guard execution error test
    let mutation_result = db_mgr.execute_sql("INSERT INTO crm_leads (contact_name, company, status, estimated_value, assigned_agent, created_month) VALUES ('Hacker', 'X', 'New', 0, 'Y', '2026-09')");
    assert!(mutation_result.is_err(), "PRAGMA query_only = ON; should block direct mutation");
}

#[test]
fn test_phase_3_postgres_mcp_connector() {
    let pg_db = EnterpriseDbManager::new_postgres("postgresql://user:pass@localhost:5432/enterprise_db".to_string());
    assert_eq!(pg_db.provider_type(), "PostgreSQL (External MCP Server)");

    let schema = pg_db.discover_schema().unwrap();
    assert_eq!(schema.tables.len(), 4);
}


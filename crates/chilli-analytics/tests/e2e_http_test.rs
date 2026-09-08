use chilli_analytics::{AnalyticsMultiAgentOrchestrator, ApiServer, EnterpriseDbManager};
use chilli_analytics::models::{ChartType, EnterpriseDomain, QueryType};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[tokio::test]
async fn test_http_query_irrelevant_query_not_enough_data() {
    let app = ApiServer::router().expect("Failed to create router");
    let req_payload = serde_json::json!({
        "query": "What is the capital of France?",
        "is_voice": false
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/query")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(req_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], false);
    assert_eq!(json["message"], "Not enough data to be processed");
}

#[tokio::test]
async fn test_http_voice_endpoint_phase_4() {
    let app = ApiServer::router().expect("Failed to create router");
    let req_payload = serde_json::json!({
        "audio_transcript": "um please show me last year sales trenduh",
        "format": "audio/webm"
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/voice")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(req_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], true);
    assert!(json["message"].as_str().unwrap().contains("Speech query normalized"));
    assert_eq!(json["data"]["dashboard"]["domain"], "ECommerce");
}

#[tokio::test]
async fn test_http_health_endpoint() {
    let app = ApiServer::router().expect("Failed to create router");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/health")
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["mcp_connected"], true);
}

#[tokio::test]
async fn test_http_schemas_endpoint() {
    let app = ApiServer::router().expect("Failed to create router");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/schemas")
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["tables"].as_array().unwrap().len() >= 4);
}

#[tokio::test]
async fn test_http_query_crm_lead_distribution() {
    let app = ApiServer::router().expect("Failed to create router");
    let req_payload = serde_json::json!({
        "query": "Show lead status distribution for this month.",
        "is_voice": false
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/query")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(req_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], true);
    assert_eq!(json["data"]["dashboard"]["domain"], "CRM");
}

#[tokio::test]
async fn test_http_query_sales_trend() {
    let app = ApiServer::router().expect("Failed to create router");
    let req_payload = serde_json::json!({
        "query": "Show monthly sales trend for the last year.",
        "is_voice": false
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/query")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(req_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], true);
    assert_eq!(json["data"]["dashboard"]["domain"], "ECommerce");
}

#[tokio::test]
async fn test_http_query_root_cause_analysis() {
    let app = ApiServer::router().expect("Failed to create router");
    let req_payload = serde_json::json!({
        "query": "Why did sales decrease last month?",
        "is_voice": false
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/query")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(req_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], true);
    assert_eq!(json["data"]["dashboard"]["domain"], "CrossDomain");
}

#[tokio::test]
async fn test_http_query_voice_filler_cleaning() {
    let app = ApiServer::router().expect("Failed to create router");
    let req_payload = serde_json::json!({
        "query": "um please show me last year sales trenduh",
        "is_voice": true
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/query")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(req_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], true);
    assert_eq!(json["data"]["dashboard"]["domain"], "ECommerce");
}

#[tokio::test]
async fn test_end_to_end_scenario_1_lead_distribution() {
    let orchestrator = AnalyticsMultiAgentOrchestrator::new().expect("Failed to init orchestrator");
    let result = orchestrator
        .execute_query("Show lead status distribution for this month.", false)
        .expect("Scenario 1 failed execution");

    assert_eq!(result.dashboard.domain, EnterpriseDomain::CRM);
    assert_eq!(result.dashboard.query_type, QueryType::Distribution);
    assert_eq!(result.dashboard.widgets[0].viz_type, ChartType::PieChart);
    assert!(result.dashboard.widgets[0].data.row_count > 0);
    assert!(!result.dashboard.recommendations.is_empty());
    assert!(result.audit_proof.starts_with("blake3:"));
}

#[tokio::test]
async fn test_end_to_end_scenario_2_sales_trend() {
    let orchestrator = AnalyticsMultiAgentOrchestrator::new().expect("Failed to init orchestrator");
    let result = orchestrator
        .execute_query("Show monthly sales trend for the last year.", false)
        .expect("Scenario 2 failed execution");

    assert_eq!(result.dashboard.domain, EnterpriseDomain::ECommerce);
    assert_eq!(result.dashboard.query_type, QueryType::Trend);
    assert_eq!(result.dashboard.widgets[0].viz_type, ChartType::LineChart);
    assert_eq!(result.dashboard.widgets[0].data.row_count, 12);
}

#[tokio::test]
async fn test_end_to_end_scenario_3_root_cause_analysis() {
    let orchestrator = AnalyticsMultiAgentOrchestrator::new().expect("Failed to init orchestrator");
    let result = orchestrator
        .execute_query("Why did sales decrease last month?", false)
        .expect("Scenario 3 failed execution");

    assert_eq!(result.dashboard.domain, EnterpriseDomain::CrossDomain);
    assert_eq!(result.dashboard.query_type, QueryType::RootCauseAnalysis);
    assert!(result.dashboard.widgets.len() >= 2);
    assert!(!result.root_cause.contributing_factors.is_empty());
    assert_eq!(result.root_cause.contributing_factors.len(), 2);
    assert!(!result.root_cause.actionable_recommendations.is_empty());
}

#[tokio::test]
async fn test_end_to_end_scenario_4_voice_input_normalization() {
    let orchestrator = AnalyticsMultiAgentOrchestrator::new().expect("Failed to init orchestrator");
    let result = orchestrator
        .execute_query("um please show me last year sales trenduh", true)
        .expect("Scenario 4 voice failed execution");

    assert_eq!(result.dashboard.domain, EnterpriseDomain::ECommerce);
    assert_eq!(result.dashboard.query_type, QueryType::Trend);
    assert_eq!(result.dashboard.widgets[0].viz_type, ChartType::LineChart);
}

#[tokio::test]
async fn test_mcp_schema_discovery_and_sql_tool() {
    let db_mgr = EnterpriseDbManager::new_in_memory().expect("Failed to init DB");
    let schema = db_mgr.discover_schema().expect("Failed schema discovery");
    assert_eq!(schema.tables.len(), 4);

    let sql_res = db_mgr
        .execute_sql("SELECT COUNT(*) AS total FROM sales_orders;")
        .expect("Failed SQL execution");

    assert_eq!(sql_res.row_count, 1);
    assert_eq!(sql_res.columns[0], "total");
}


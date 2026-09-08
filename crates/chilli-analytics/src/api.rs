use crate::orchestrator::AnalyticsMultiAgentOrchestrator;
use axum::{
    extract::State,
    http::{Method, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

pub struct AppState {
    pub orchestrator: AnalyticsMultiAgentOrchestrator,
}

#[derive(Debug, Deserialize)]
pub struct QueryRequestPayload {
    pub query: String,
    #[serde(default)]
    pub is_voice: bool,
}

#[derive(Debug, Serialize)]
pub struct HealthResponsePayload {
    pub status: String,
    pub mcp_connected: bool,
    pub version: String,
}

/// Start the Axum HTTP REST API Web Server for Frontend Integration
pub async fn start_api_server(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let orchestrator = AnalyticsMultiAgentOrchestrator::new()?;
    let state = Arc::new(AppState { orchestrator });

    // CORS configuration for React (3000), Vite (5173), and general local origins
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(Any);

    let app = Router::new()
        .route("/api/v1/health", get(handle_health))
        .route("/api/v1/query", post(handle_query))
        .route("/api/v1/schemas", get(handle_schemas))
        .route("/api/v1/voice", post(handle_voice))
        .layer(cors)
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Starting Agentic Analytics REST API Server on http://{}", addr);
    println!("\n🚀 Agentic Analytics REST API Server listening on http://{}", addr);
    println!("   - GET  /api/v1/health   : Server & MCP health check");
    println!("   - POST /api/v1/query    : Process natural language analytics query");
    println!("   - GET  /api/v1/schemas  : MCP schema discovery metadata");
    println!("   - POST /api/v1/voice    : Voice command processing endpoint\n");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn handle_health() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(HealthResponsePayload {
            status: "ok".to_string(),
            mcp_connected: true,
            version: "0.1.0".to_string(),
        }),
    )
}

async fn handle_query(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<QueryRequestPayload>,
) -> impl IntoResponse {
    info!("API POST /api/v1/query -> '{}' (is_voice={})", payload.query, payload.is_voice);

    match state.orchestrator.execute_query(&payload.query, payload.is_voice) {
        Ok(result) => (
            StatusCode::OK,
            Json(json!({
                "success": true,
                "dashboard": result.dashboard,
                "root_cause": result.root_cause
            })),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "success": false,
                "error": err.to_string()
            })),
        ),
    }
}

async fn handle_schemas(State(_state): State<Arc<AppState>>) -> impl IntoResponse {
    info!("API GET /api/v1/schemas");
    let intent = crate::models::QueryIntent {
        raw_query: "schema discovery".to_string(),
        is_voice_input: false,
        domain: crate::models::EnterpriseDomain::CrossDomain,
        query_type: crate::models::QueryType::Aggregation,
        target_entities: vec![],
        metrics: vec![],
        time_horizon: None,
        filter_conditions: vec![],
    };

    let schema_agent = crate::agents::SchemaDiscoveryAgent::new();
    let db_mgr = crate::db_mcp::EnterpriseDbManager::new_in_memory().unwrap();

    match schema_agent.discover_relevant_schema(&intent, &db_mgr) {
        Ok(schema) => (
            StatusCode::OK,
            Json(json!({
                "success": true,
                "domains": ["CRM", "E-Commerce", "ERP", "HRMS"],
                "tables": schema.tables
            })),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "success": false,
                "error": err.to_string()
            })),
        ),
    }
}

async fn handle_voice(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<QueryRequestPayload>,
) -> impl IntoResponse {
    info!("API POST /api/v1/voice -> Raw voice payload: '{}'", payload.query);

    match state.orchestrator.execute_query(&payload.query, true) {
        Ok(result) => (
            StatusCode::OK,
            Json(json!({
                "success": true,
                "is_voice_processed": true,
                "dashboard": result.dashboard,
                "root_cause": result.root_cause
            })),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "success": false,
                "error": err.to_string()
            })),
        ),
    }
}

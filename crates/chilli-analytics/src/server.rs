use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

use crate::db_mcp::EnterpriseDbManager;
use crate::models::SchemaMetadata;
use crate::orchestrator::{AnalyticsMultiAgentOrchestrator, AnalyticsResult};

/// Shared server state holding the multi-agent orchestrator
pub struct AppState {
    pub orchestrator: AnalyticsMultiAgentOrchestrator,
    pub db_manager: EnterpriseDbManager,
}

/// Request payload for query execution
#[derive(Debug, Deserialize)]
pub struct QueryRequest {
    pub query: String,
    pub is_voice: Option<bool>,
}

/// Response payload for health check
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub mcp_connected: bool,
    pub version: String,
}

/// Response payload for query execution
#[derive(Debug, Serialize)]
pub struct QueryApiResponse {
    pub success: bool,
    pub message: String,
    pub data: Option<AnalyticsResult>,
}

pub struct ApiServer;

impl ApiServer {
    /// Start the Axum HTTP REST server on specified port
    pub async fn run(port: u16) -> Result<(), Box<dyn std::error::Error>> {
        let orchestrator = AnalyticsMultiAgentOrchestrator::new()?;
        let db_manager = EnterpriseDbManager::new_in_memory()?;

        let shared_state = Arc::new(AppState {
            orchestrator,
            db_manager,
        });

        let cors = CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any);

        let app = Router::new()
            .route("/api/v1/health", get(health_handler))
            .route("/api/v1/query", post(query_handler))
            .route("/api/v1/schemas", get(schemas_handler))
            .layer(cors)
            .with_state(shared_state);

        let addr = SocketAddr::from(([0, 0, 0, 0], port));
        info!("🚀 CHILLI ANALYTICS REST API SERVER LISTENING ON http://localhost:{}", port);
        println!("==========================================================================");
        println!("  🚀 BACKEND REST API SERVER ONLINE: http://localhost:{}", port);
        println!("  🔌 FRONTEND ENDPOINT  : POST http://localhost:{}/api/v1/query", port);
        println!("  💚 HEALTH CHECK       : GET  http://localhost:{}/api/v1/health", port);
        println!("  🔍 SCHEMA DISCOVERY   : GET  http://localhost:{}/api/v1/schemas", port);
        println!("==========================================================================\n");

        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }
}

async fn health_handler() -> (StatusCode, Json<HealthResponse>) {
    (
        StatusCode::OK,
        Json(HealthResponse {
            status: "ok".to_string(),
            mcp_connected: true,
            version: env!("CARGO_PKG_VERSION").to_string(),
        }),
    )
}

async fn query_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<QueryRequest>,
) -> (StatusCode, Json<QueryApiResponse>) {
    info!("Received HTTP query request: '{}'", payload.query);
    let is_voice = payload.is_voice.unwrap_or(false);

    match state.orchestrator.execute_query(&payload.query, is_voice) {
        Ok(result) => (
            StatusCode::OK,
            Json(QueryApiResponse {
                success: true,
                message: "Query processed successfully through 8-agent pipeline".to_string(),
                data: Some(result),
            }),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(QueryApiResponse {
                success: false,
                message: format!("Pipeline execution error: {}", err),
                data: None,
            }),
        ),
    }
}

async fn schemas_handler(
    State(state): State<Arc<AppState>>,
) -> (StatusCode, Json<SchemaMetadata>) {
    let schema = state.db_manager.discover_schema().unwrap_or_else(|_| SchemaMetadata { tables: vec![] });
    (StatusCode::OK, Json(schema))
}

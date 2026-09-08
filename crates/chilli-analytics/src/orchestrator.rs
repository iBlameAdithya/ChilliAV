use crate::agents::{
    DashboardGenerationAgent, InsightAndRecommendationAgent, IntentUnderstandingAgent,
    SchemaDiscoveryAgent, SqlGenerationAgent, SqlValidationAgent, VisualizationSelectionAgent,
    VoiceProcessingAgent,
};
use crate::db_mcp::EnterpriseDbManager;
use crate::models::{DashboardSpec, RootCauseAnalysis, SqlExecutionResult};
use tracing::info;

pub struct AnalyticsResult {
    pub dashboard: DashboardSpec,
    pub root_cause: RootCauseAnalysis,
}

/// Multi-Agent Orchestrator for Enterprise Analytics
pub struct AnalyticsMultiAgentOrchestrator {
    voice_agent: VoiceProcessingAgent,
    intent_agent: IntentUnderstandingAgent,
    schema_agent: SchemaDiscoveryAgent,
    sql_gen_agent: SqlGenerationAgent,
    sql_val_agent: SqlValidationAgent,
    viz_agent: VisualizationSelectionAgent,
    dashboard_agent: DashboardGenerationAgent,
    insight_agent: InsightAndRecommendationAgent,
    db_manager: EnterpriseDbManager,
}

impl AnalyticsMultiAgentOrchestrator {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let db_manager = EnterpriseDbManager::new_in_memory()?;
        Ok(Self {
            voice_agent: VoiceProcessingAgent::new(),
            intent_agent: IntentUnderstandingAgent::new(),
            schema_agent: SchemaDiscoveryAgent::new(),
            sql_gen_agent: SqlGenerationAgent::new(),
            sql_val_agent: SqlValidationAgent::new(),
            viz_agent: VisualizationSelectionAgent::new(),
            dashboard_agent: DashboardGenerationAgent::new(),
            insight_agent: InsightAndRecommendationAgent::new(),
            db_manager,
        })
    }

    /// Process a natural language or voice analytical request end-to-end through the 8 AI agents
    pub fn execute_query(
        &self,
        raw_query: &str,
        is_voice: bool,
    ) -> Result<AnalyticsResult, Box<dyn std::error::Error>> {
        info!("--- STARTING MULTI-AGENT ANALYTICS PIPELINE ---");

        // Step 1: Voice Processing Agent
        let (clean_query, is_voice_processed) = self.voice_agent.process_input(raw_query, is_voice);
        info!("Step 1 [Voice Processing Agent]: Transcribed text: '{}'", clean_query);

        // Step 2: Intent Understanding Agent
        let intent = self.intent_agent.analyze_intent(&clean_query, is_voice_processed);
        info!("Step 2 [Intent Understanding Agent]: Intent domain: {:?}, query type: {:?}", intent.domain, intent.query_type);

        // Step 3: Schema Discovery Agent (via MCP)
        let schema = self.schema_agent.discover_relevant_schema(&intent, &self.db_manager)?;
        info!("Step 3 [Schema Discovery Agent]: Discovered {} tables", schema.tables.len());

        // Step 4: SQL Generation Agent
        let sql_payload = self.sql_gen_agent.generate_sql(&intent, &schema)?;
        info!("Step 4 [SQL Generation Agent]: Generated Primary SQL: '{}'", sql_payload.primary_sql);

        // Step 5: SQL Validation Agent
        let validated_primary_sql = self.sql_val_agent.validate_sql(&sql_payload.primary_sql, &schema)?;
        info!("Step 5 [SQL Validation Agent]: Primary SQL query validated successfully.");

        let primary_result = self.db_manager.execute_sql(&validated_primary_sql)?;
        info!("Step 5b [Data Execution Layer]: Executed query, retrieved {} rows", primary_result.row_count);

        let mut auxiliary_results: Vec<(String, SqlExecutionResult)> = Vec::new();
        for (label, aux_sql) in sql_payload.auxiliary_sqls {
            if let Ok(val_aux) = self.sql_val_agent.validate_sql(&aux_sql, &schema) {
                if let Ok(res) = self.db_manager.execute_sql(&val_aux) {
                    auxiliary_results.push((label, res));
                }
            }
        }

        // Step 6: Visualization Selection Agent
        let viz_config = self.viz_agent.select_visualization(&intent, &primary_result);
        info!("Step 6 [Visualization Selection Agent]: Selected Chart Type: {}", viz_config.chart_type);

        // Step 7: Dashboard Generation Agent
        let mut dashboard = self.dashboard_agent.generate_dashboard(
            &intent,
            viz_config,
            primary_result,
            auxiliary_results,
        );
        info!("Step 7 [Dashboard Generation Agent]: Assembled Dashboard Spec '{}' with {} widgets", dashboard.title, dashboard.widgets.len());

        // Step 8: Insight & Recommendation Agent
        let root_cause = self.insight_agent.generate_insights(&intent, &mut dashboard);
        info!("Step 8 [Insight & Recommendation Agent]: Generated executive summary and {} recommendations", dashboard.recommendations.len());

        info!("--- MULTI-AGENT ANALYTICS PIPELINE COMPLETE ---");
        Ok(AnalyticsResult {
            dashboard,
            root_cause,
        })
    }
}

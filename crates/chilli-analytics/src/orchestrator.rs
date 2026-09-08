use crate::agents::{
    DashboardGenerationAgent, InsightAndRecommendationAgent, IntentUnderstandingAgent,
    SchemaDiscoveryAgent, SqlGenerationAgent, SqlValidationAgent, VisualizationSelectionAgent,
    VoiceProcessingAgent,
};
use crate::db_mcp::EnterpriseDbManager;
use crate::models::{DashboardSpec, RootCauseAnalysis, SqlExecutionResult};
use chilli_policy::proof::generate_execution_proof;
use std::time::Instant;
use tracing::info;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsResult {
    pub dashboard: DashboardSpec,
    pub root_cause: RootCauseAnalysis,
    pub audit_proof: String,
}

/// Auto-Immune Loop Extermination Guard (Circuit Breaker)
/// Prevents infinite reasoning loops and excessive execution times
pub struct LoopDetectorGuard {
    max_steps: usize,
    max_duration_ms: u128,
    start_time: Instant,
    step_count: usize,
}

impl LoopDetectorGuard {
    pub fn new(max_steps: usize, max_duration_ms: u128) -> Self {
        Self {
            max_steps,
            max_duration_ms,
            start_time: Instant::now(),
            step_count: 0,
        }
    }

    pub fn tick(&mut self, step_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.step_count += 1;
        let elapsed = self.start_time.elapsed().as_millis();
        if self.step_count > self.max_steps {
            return Err(format!("Circuit Breaker: Maximum execution step count ({}) exceeded at '{}'", self.max_steps, step_name).into());
        }
        if elapsed > self.max_duration_ms {
            return Err(format!("Circuit Breaker: Execution timeout ({}ms > {}ms) exceeded at '{}'", elapsed, self.max_duration_ms, step_name).into());
        }
        Ok(())
    }
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
        let overall_start = Instant::now();
        let mut loop_guard = LoopDetectorGuard::new(12, 5000);
        info!("--- STARTING MULTI-AGENT ANALYTICS PIPELINE ---");

        // Step 1: Voice Processing Agent
        loop_guard.tick("Step 1: Voice Processing")?;
        let t1 = Instant::now();
        let (clean_query, is_voice_processed) = self.voice_agent.process_input(raw_query, is_voice);
        info!("Step 1 [Voice Processing Agent] ({:.2} ms): Transcribed text: '{}'", t1.elapsed().as_secs_f64() * 1000.0, clean_query);

        // Step 2: Speculative Swarm & Intent Understanding Agent
        loop_guard.tick("Step 2: Intent Understanding")?;
        let t2 = Instant::now();
        info!("⚡ [Speculative Swarm Engine]: Pre-warming domain index routing...");
        let intent = self.intent_agent.analyze_intent(&clean_query, is_voice_processed);
        info!("Step 2 [Intent Understanding Agent] ({:.2} ms): Intent domain: {:?}, query type: {:?}", t2.elapsed().as_secs_f64() * 1000.0, intent.domain, intent.query_type);

        if intent.domain == crate::models::EnterpriseDomain::Unknown {
            info!("Intent Understanding Agent: Query is irrelevant to available enterprise datasets.");
            return Err("Not enough data to be processed".into());
        }

        // Step 3: Schema Discovery Agent (via MCP)
        loop_guard.tick("Step 3: Schema Discovery")?;
        let t3 = Instant::now();
        let schema = self.schema_agent.discover_relevant_schema(&intent, &self.db_manager)?;
        info!("Step 3 [Schema Discovery Agent] ({:.2} ms): Discovered {} tables", t3.elapsed().as_secs_f64() * 1000.0, schema.tables.len());

        // Step 4: SQL Generation Agent
        loop_guard.tick("Step 4: SQL Generation")?;
        let t4 = Instant::now();
        let sql_payload = self.sql_gen_agent.generate_sql(&intent, &schema)?;
        info!("Step 4 [SQL Generation Agent] ({:.2} ms): Generated Primary SQL: '{}'", t4.elapsed().as_secs_f64() * 1000.0, sql_payload.primary_sql);

        // Step 5: SQL Validation & Data Execution (Fan-Out Agent Pattern)
        loop_guard.tick("Step 5: SQL Validation & Execution")?;
        let t5 = Instant::now();
        let validated_primary_sql = self.sql_val_agent.validate_sql(&sql_payload.primary_sql, &schema)?;
        info!("Step 5 [SQL Validation Agent] ({:.2} ms): Primary SQL query validated successfully.", t5.elapsed().as_secs_f64() * 1000.0);

        let t5b = Instant::now();
        let primary_result = self.db_manager.execute_sql(&validated_primary_sql)?;
        info!("Step 5b [Data Execution Layer] ({:.2} ms): Primary dataset query executed, retrieved {} rows", t5b.elapsed().as_secs_f64() * 1000.0, primary_result.row_count);

        let t_fanout = Instant::now();
        let mut auxiliary_results: Vec<(String, SqlExecutionResult)> = Vec::new();
        if !sql_payload.auxiliary_sqls.is_empty() {
            info!("⚡ [Fan-Out Swarm Agent Engine]: Fanning out {} cross-domain sub-agent queries in parallel...", sql_payload.auxiliary_sqls.len());

            use std::sync::{Arc, Mutex};
            use std::thread;

            let aux_results = Arc::new(Mutex::new(Vec::new()));
            let mut handles = Vec::new();

            for (label, aux_sql) in sql_payload.auxiliary_sqls {
                let db_manager = self.db_manager.clone();
                let sql_val_agent = SqlValidationAgent::new();
                let schema = schema.clone();
                let aux_results = Arc::clone(&aux_results);

                let handle = thread::spawn(move || {
                    let sub_t = Instant::now();
                    if let Ok(val_aux) = sql_val_agent.validate_sql(&aux_sql, &schema) {
                        if let Ok(res) = db_manager.execute_sql(&val_aux) {
                            info!("   ↳ [Sub-Agent Worker: '{}'] ({:.2} ms): Completed execution ({} rows)", label, sub_t.elapsed().as_secs_f64() * 1000.0, res.row_count);
                            let mut results = aux_results.lock().unwrap();
                            results.push((label, res));
                        }
                    }
                });
                handles.push(handle);
            }

            for handle in handles {
                let _ = handle.join();
            }

            let locked_res = aux_results.lock().unwrap();
            auxiliary_results = locked_res.clone();
            info!("Step 5c [Fan-Out Parallel Swarm] ({:.2} ms): All {} cross-domain sub-agent tasks joined successfully.", t_fanout.elapsed().as_secs_f64() * 1000.0, auxiliary_results.len());
        }

        // Step 6: Visualization Selection Agent
        loop_guard.tick("Step 6: Visualization Selection")?;
        let t6 = Instant::now();
        let viz_config = self.viz_agent.select_visualization(&intent, &primary_result);
        info!("Step 6 [Visualization Selection Agent] ({:.2} ms): Selected Chart Type: {}", t6.elapsed().as_secs_f64() * 1000.0, viz_config.chart_type);

        // Step 7: Dashboard Generation Agent
        loop_guard.tick("Step 7: Dashboard Generation")?;
        let t7 = Instant::now();
        let mut dashboard = self.dashboard_agent.generate_dashboard(
            &intent,
            viz_config,
            primary_result,
            auxiliary_results,
        );
        info!("Step 7 [Dashboard Generation Agent] ({:.2} ms): Assembled Dashboard Spec '{}' with {} widgets", t7.elapsed().as_secs_f64() * 1000.0, dashboard.title, dashboard.widgets.len());

        // Step 8: Insight & Recommendation Agent
        loop_guard.tick("Step 8: Insight & Recommendation")?;
        let t8 = Instant::now();
        let root_cause = self.insight_agent.generate_insights(&intent, &mut dashboard);
        info!("Step 8 [Insight & Recommendation Agent] ({:.2} ms): Generated executive summary and {} recommendations", t8.elapsed().as_secs_f64() * 1000.0, dashboard.recommendations.len());

        // Cryptographic Proof of Execution Token Generation (SOC2 / ISO 27001 Compliance)
        let audit_proof = generate_execution_proof(
            &clean_query,
            &validated_primary_sql,
            &format!("{:?}", intent.domain),
            &dashboard.generated_at,
        );
        info!("🔐 [Cryptographic Proof Engine]: Generated BLAKE3 SOC2 execution proof: '{}'", audit_proof);

        let total_ms = overall_start.elapsed().as_secs_f64() * 1000.0;
        info!("--- MULTI-AGENT ANALYTICS PIPELINE COMPLETE (Total Latency: {:.2} ms) ---", total_ms);
        Ok(AnalyticsResult {
            dashboard,
            root_cause,
            audit_proof,
        })
    }
}


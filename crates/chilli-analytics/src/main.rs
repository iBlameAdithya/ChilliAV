use chilli_analytics::{AnalyticsMultiAgentOrchestrator, ApiServer, DashboardRenderer};
use std::env;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing subscriber for clean pipeline logs
    tracing_subscriber::fmt::init();

    let args: Vec<String> = env::args().collect();
    let run_cli = args.iter().any(|arg| arg == "--cli");

    if !run_cli {
        let port: u16 = env::var("PORT")
            .unwrap_or_else(|_| "8080".to_string())
            .parse()
            .unwrap_or(8080);
        ApiServer::run(port).await?;
        return Ok(());
    }

    println!("==========================================================================================");
    println!("     AGENTIC ANALYTICS PLATFORM FOR ENTERPRISE APPLICATIONS USING MCP & MULTI-AGENT AI    ");
    println!("==========================================================================================");

    let orchestrator = AnalyticsMultiAgentOrchestrator::new()?;

    // Scenario 1: Lead Status Distribution (CRM Domain)
    println!("\n>>> [SCENARIO 1] USER COMMAND: 'Show lead status distribution for this month.'");
    let res1 = orchestrator.execute_query("Show lead status distribution for this month.", false)?;
    DashboardRenderer::render_terminal_report(&res1);
    let html_path_1 = Path::new("scenario1_crm_dashboard.html");
    DashboardRenderer::render_html_file(&res1, html_path_1)?;
    println!("[+] Exported HTML Dashboard for Scenario 1 -> {}", html_path_1.display());

    // Scenario 2: Monthly Sales Trend (E-Commerce / ERP Domain)
    println!("\n>>> [SCENARIO 2] USER COMMAND: 'Show monthly sales trend for the last year.'");
    let res2 = orchestrator.execute_query("Show monthly sales trend for the last year.", false)?;
    DashboardRenderer::render_terminal_report(&res2);
    let html_path_2 = Path::new("scenario2_sales_trend_dashboard.html");
    DashboardRenderer::render_html_file(&res2, html_path_2)?;
    println!("[+] Exported HTML Dashboard for Scenario 2 -> {}", html_path_2.display());

    // Scenario 3: Root-Cause Analysis (Cross-Domain Analysis)
    println!("\n>>> [SCENARIO 3] USER COMMAND: 'Why did sales decrease last month?'");
    let res3 = orchestrator.execute_query("Why did sales decrease last month?", false)?;
    DashboardRenderer::render_terminal_report(&res3);
    let html_path_3 = Path::new("scenario3_root_cause_dashboard.html");
    DashboardRenderer::render_html_file(&res3, html_path_3)?;
    println!("[+] Exported HTML Dashboard for Scenario 3 -> {}", html_path_3.display());

    // Scenario 4: Voice Input Ingestion
    println!("\n>>> [SCENARIO 4] VOICE COMMAND: 'um please show me last year sales trenduh'");
    let res4 = orchestrator.execute_query("um please show me last year sales trenduh", true)?;
    DashboardRenderer::render_terminal_report(&res4);

    println!("\nAll 3 core scenarios executed successfully through the 8 AI agents and MCP Data Access Layer!");
    Ok(())
}


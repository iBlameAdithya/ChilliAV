# Architecture & Specification: Agentic Analytics Platform for Enterprise Applications Using MCP and Multi-Agent AI

## Executive Overview

The **Agentic Analytics Platform** is a reusable, application-independent analytics solution engineered to bridge the gap between non-technical business users and complex enterprise databases (ERP, CRM, HRMS, E-Commerce). By leveraging the **Model Context Protocol (MCP)** for standardized schema discovery and data connectivity, combined with a **Multi-Agent AI Architecture**, the platform converts voice and natural language commands into validated SQL queries, dynamic chart selections, interactive web dashboards, and executive root-cause analysis reports.

---

## System Architecture

```
                               ┌─────────────────────────────────────────────────────────┐
                               │                    USER INTERFACE                       │
                               │        Voice Command (STT) / Natural Language           │
                               └───────────────────────────┬─────────────────────────────┘
                                                           │
                                                           ▼
┌──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
│                                           AI MULTI-AGENT ORCHESTRATION LAYER                                         │
│                                                                                                                      │
│  ┌───────────────────────┐    ┌───────────────────────┐    ┌───────────────────────┐    ┌───────────────────────┐   │
│  │ Voice Processing      │───▶│ Intent Understanding │───▶│ Schema Discovery      │───▶│ SQL Generation        │   │
│  │ Agent                 │    │ Agent                 │    │ Agent (MCP Client)    │    │ Agent                 │   │
│  └───────────────────────┘    └───────────────────────┘    └───────────────────────┘    └───────────────────────┘   │
│                                                                                                     │                │
│                                                                                                     ▼                │
│  ┌───────────────────────┐    ┌───────────────────────┐    ┌───────────────────────┐    ┌───────────────────────┐   │
│  │ Insight &             │◀───│ Dashboard Generation  │◀───│ Visualization         │◀───│ SQL Validation        │   │
│  │ Recommendation Agent  │    │ Agent                 │    │ Selection Agent       │    │ Agent (Security Gate) │   │
│  └───────────────────────┘    └───────────────────────┘    └───────────────────────┘    └───────────────────────┘   │
└─────────────────────────────────────────────┬────────────────────────────────────────────────────────────────────────┘
                                              │
                                              ▼
┌──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
│                                       MCP DATA ACCESS & ENTERPRISE INTEGRATION                                      │
│                                                                                                                      │
│   ┌─────────────────────┐   ┌─────────────────────┐   ┌─────────────────────┐   ┌─────────────────────┐              │
│   │     CRM Database    │   │  E-Commerce Database│   │     ERP Database    │   │    HRMS Database    │              │
│   │ (Leads, Deals, Reps)│   │  (Sales Orders, Cat)│   │ (Inventory, AdSpend)│   │(Staff, Salaries, HR)│              │
│   └─────────────────────┘   └─────────────────────┘   └─────────────────────┘   └─────────────────────┘              │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

---

## Key Platform Layers & Components

### 1. Data Access Layer (MCP Engine)
- **Standardized Connectivity**: Standard MCP protocol tools (`mcp_enterprise_discover_schema`, `mcp_enterprise_execute_sql`) decouple agent logic from database implementations.
- **Automated Metadata Management**: Dynamically retrieves table names, column types, descriptions, foreign keys, and primary keys across CRM, ERP, HRMS, and E-Commerce.
- **Read-Only Safety**: Enforces read-only connection semantics to guarantee data integrity.

### 2. Specialized 8-Agent Multi-Agent Layer
1. **Voice Processing Agent (`VoiceProcessingAgent`)**:
   - Ingests speech transcripts or text queries.
   - Cleans acoustic filler words (`um`, `uh`, `please show me`), normalizes domain vocabulary, and extracts acoustic metadata.
2. **Intent Understanding Agent (`IntentUnderstandingAgent`)**:
   - Classifies target domain (`CRM`, `ERP`, `HRMS`, `ECommerce`, `CrossDomain`).
   - Categorizes analytical query types (`Distribution`, `Trend`, `RootCauseAnalysis`, `Aggregation`).
   - Identifies targets, time horizons, and filter conditions.
3. **Schema Discovery Agent (`SchemaDiscoveryAgent`)**:
   - Interrogates MCP schema endpoints to discover relevant tables and column attributes matching intent.
4. **SQL Generation Agent (`SqlGenerationAgent`)**:
   - Dynamically constructs dialect-optimized SQL queries (both primary queries and auxiliary cross-domain queries for root-cause correlation).
5. **SQL Validation Agent (`SqlValidationAgent`)**:
   - Security sandbox gate that rejects mutating SQL keywords (`INSERT`, `UPDATE`, `DELETE`, `DROP`, `ALTER`, `TRUNCATE`), verifies syntax balance, and prevents SQL injection.
6. **Visualization Selection Agent (`VisualizationSelectionAgent`)**:
   - Evaluates row counts, column data types, and query types to select the optimal chart:
     - **Pie Chart**: Category distribution ($\le 8$ items).
     - **Line Chart**: Time-series revenue/metric trends over time.
     - **Bar Chart**: High-cardinality comparisons.
     - **KPI Card**: Single aggregate metrics.
     - **Data Table**: Tabular multi-column detailed views.
7. **Dashboard Generation Agent (`DashboardGenerationAgent`)**:
   - Assembles multi-widget dashboard specifications, placing charts, query execution logs, and executive summary narratives.
8. **Insight & Recommendation Agent (`InsightAndRecommendationAgent`)**:
   - Conducts automated cross-domain anomaly detection and root-cause analysis (e.g. correlating sales drops with ad spend cuts and inventory stock-outs) and generates actionable business recommendations.

### 3. Presentation Layer
- **Interactive Web Dashboards**: Standalone HTML5 pages with embedded Chart.js rendering animated charts.
- **Terminal ASCII Reports**: Formatted CLI output displaying executive summaries, SQL execution times, and recommendations.

---

## Core Scenario Execution Workflow

### Scenario 1: CRM Lead Distribution
- **Query**: *"Show lead status distribution for this month."*
- **Target Domain**: CRM (`crm_leads`)
- **Generated SQL**:
  ```sql
  SELECT status, COUNT(*) AS count, SUM(estimated_value) AS total_value
  FROM crm_leads
  WHERE created_month = '2026-09'
  GROUP BY status ORDER BY count DESC;
  ```
- **Selected Viz**: Pie Chart
- **Output**: CRM Lead Status Dashboard

### Scenario 2: E-Commerce Sales Trend
- **Query**: *"Show monthly sales trend for the last year."*
- **Target Domain**: E-Commerce (`sales_orders`)
- **Generated SQL**:
  ```sql
  SELECT month, SUM(amount) AS total_sales
  FROM sales_orders
  GROUP BY month ORDER BY month ASC;
  ```
- **Selected Viz**: Line Chart
- **Output**: 12-Month Sales Trend Dashboard

### Scenario 3: Cross-Domain Root-Cause Analysis
- **Query**: *"Why did sales decrease last month?"*
- **Target Domain**: Cross-Domain Enterprise (Sales + Marketing + ERP Inventory)
- **Primary SQL**:
  ```sql
  SELECT month, SUM(amount) AS total_sales
  FROM sales_orders WHERE month IN ('2026-06', '2026-07', '2026-08', '2026-09')
  GROUP BY month ORDER BY month ASC;
  ```
- **Auxiliary SQLs**:
  - `SELECT month, ad_spend, leads_generated FROM marketing_campaigns ...`
  - `SELECT sku, product_name, stock_out_events FROM erp_inventory WHERE month = '2026-08';`
- **Diagnosed Causes**:
  1. Digital Ad Spend cut by **67.2%** in August 2026 ($55k $\rightarrow$ $18k$).
  2. Inventory stock-out events spiked to **42 events** for primary software SKU.
- **Output**: Multi-Widget Analytical Panel + Strategic Actionable Business Recommendations

---

## Addressing Research Gaps

| Research Gap Highlighted in Statement | Platform Solution Implemented |
| :--- | :--- |
| **MCP-based Universal Integration** | Standardized MCP tool handlers (`mcp_enterprise_discover_schema`, `mcp_enterprise_execute_sql`) allow connecting any database without changing AI agent logic. |
| **Multi-Agent Orchestration** | 8 specialized AI agents orchestrated sequentially with strict boundaries (validation, visualization, insights). |
| **Voice-Driven Generation** | `VoiceProcessingAgent` strips acoustic fillers, normalizes natural speech, and feeds clean text into intent parser. |
| **Automated Root-Cause Analysis** | `InsightAndRecommendationAgent` correlates multi-dataset anomalies across marketing spend, inventory levels, and CRM pipelines. |
| **Application-Independent Architecture** | Decoupled architecture supporting CRM, ERP, HRMS, E-Commerce seamlessly in a single platform. |

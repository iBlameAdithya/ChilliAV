# ChilliAV - Agentic Analytics Platform for Enterprise Applications

> **Powered by Model Context Protocol (MCP) & Multi-Agent AI (Rust Core Engine)**

`ChilliAV` is an application-independent, enterprise-grade Agentic Analytics Platform written in high-performance Rust. It allows non-technical business users to query complex enterprise databases (ERP, CRM, HRMS, E-Commerce) using natural language or voice commands, automatically generating SQL queries, visualizations, dynamic dashboards, and root-cause diagnostic analyses.

---

## 🏗️ Core Architecture & Agent Workflow

The backend is structured into specialized crates with an 8-Agent pipeline inside `chilli-analytics`:

```
[Voice/Text Input] 
       │
       ▼
1. Voice Processing Agent ──► 2. Intent Understanding Agent ──► 3. Schema Discovery Agent (MCP)
                                                                            │
                                                                            ▼
6. Viz Selection Agent ◄── 5. SQL Validation Agent ◄── 4. SQL Generation Agent
       │
       ▼
7. Dashboard Generation Agent ──► 8. Insight & Recommendation Agent ──► [DashboardSpec Payload (JSON)]
```

---

## 🔌 API & Frontend Integration Specification

If you are building the **Frontend (React, Next.js, Vue, Tailwind, Chart.js / Recharts)**, this is the JSON payload structure returned by the backend `AnalyticsMultiAgentOrchestrator`.

### 1. Execute Query Endpoint Request

```json
{
  "query": "Why did sales decrease last month?",
  "is_voice": true
}
```

### 2. Analytics Output Payload (`DashboardSpec` + `RootCauseAnalysis`)

The backend returns a unified `AnalyticsResult` containing everything needed to render the UI:

```json
{
  "dashboard": {
    "dashboard_id": "dash_crm_9921",
    "title": "Cross-Domain Enterprise Analytical Dashboard",
    "domain": "CrossDomain",
    "query_type": "RootCauseAnalysis",
    "executive_summary": "Correlated sales revenue decline in August 2026 against digital ad spend and inventory outages.",
    "generated_at": "2026-09-08 14:00:00 UTC",
    "widgets": [
      {
        "id": "widget_1",
        "title": "Why did sales decrease last month?",
        "viz_type": "LineChart",
        "config": {
          "chart_type": "LineChart",
          "title": "Monthly Sales Trend",
          "x_axis_label": "month",
          "y_axis_label": "total_sales",
          "category_column": "month",
          "value_column": "total_sales",
          "color_scheme": ["#3b82f6", "#10b981"]
        },
        "sql_query": "SELECT month, SUM(amount) AS total_sales FROM sales_orders WHERE month IN ('2026-06', '2026-07', '2026-08', '2026-09') GROUP BY month ORDER BY month ASC;",
        "data": {
          "sql_query": "SELECT month, SUM(amount) AS total_sales ...",
          "columns": ["month", "total_sales"],
          "rows": [
            ["2026-06", 185000.0],
            ["2026-07", 190000.0],
            ["2026-08", 110000.0],
            ["2026-09", 175000.0]
          ],
          "row_count": 4,
          "execution_time_ms": 0
        }
      }
    ],
    "recommendations": [
      "Re-instate Digital Marketing budget to baseline level ($50,000+/month).",
      "Implement automated reorder threshold triggers for SKU-100 in ERP."
    ]
  },
  "root_cause": {
    "primary_issue": "Sales revenue dropped by 42.1% in August 2026 ($190,000 -> $110,000)",
    "observation_period": "August 2026",
    "primary_metric_change": "-$80,000 (-42.1%)",
    "root_cause_summary": "The August 2026 sales drop was primarily driven by a 67.2% reduction in digital ad spend coupled with severe stock-out events for primary software licenses in ERP inventory.",
    "contributing_factors": [
      {
        "domain": "ERP",
        "metric": "Marketing Ad Spend",
        "change_percentage": -67.2,
        "narrative": "Digital ad budget was slashed from $55,000 in July to $18,000 in August 2026, leading to a 71% reduction in new qualified lead generation."
      }
    ],
    "actionable_recommendations": [
      "Re-instate Digital Marketing budget to baseline level ($50,000+/month).",
      "Implement automated reorder threshold triggers for SKU-100."
    ]
  }
}
```

---

## 🎨 Supported Visualization Types (`ChartType`)

The frontend should support rendering the following chart components based on `viz_type`:

| `viz_type` String | Recommended Frontend Component |
| :--- | :--- |
| `"PieChart"` | Recharts / Chart.js Pie or Donut Chart |
| `"BarChart"` | Vertical or Horizontal Bar Chart |
| `"LineChart"` | Smooth Area / Line Chart for timeline data |
| `"KpiCard"` | Metric Card with big bold numbers + percentage delta |
| `"DataTable"` | Paginated Data Table with column headers |
| `"AreaChart"` | Stacked / Filled Area Chart |

---

## 🚀 How to Run the Backend

### Prerequisites
* Rust toolchain (1.75+) installed on system.

### Build and Run Demo Pipelines
```bash
# Run the end-to-end multi-agent analytics pipeline CLI
cargo run --bin chilli-analytics

# Run tests across workspace crates
cargo test --workspace
```

---

## 📦 Workspace Crates Overview

* `chilli-analytics`: Main application crate containing the 8 AI agents, MCP enterprise DB connectors, and dashboard generator.
* `chilli-mcp`: Pure Rust Model Context Protocol (MCP) host & client tool bridge.
* `chilli-orchestrator`: Dynamic Task Graph (DAG) executor, parallel execution engine & blackboard memory.
* `chilli-core`: Multi-agent execution loop, loop detector guard, and AST verifiers.
* `chilli-policy`: Credential leakage filter and safe path/command execution policies.
* `chilli-sandbox`: Process isolation and git worktree sandboxing.
* `chilli-model`: Provider-agnostic LLM router (Anthropic, OpenAI, Local models) with SSE streaming.

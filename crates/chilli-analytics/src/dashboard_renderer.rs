use crate::orchestrator::AnalyticsResult;
use std::fs;
use std::path::Path;

pub struct DashboardRenderer;

impl DashboardRenderer {
    /// Render formatted ASCII report to terminal
    pub fn render_terminal_report(result: &AnalyticsResult) {
        let dash = &result.dashboard;
        let rc = &result.root_cause;

        println!("\n==========================================================================================");
        println!("                         ENTERPRISE AGENTIC ANALYTICS DASHBOARD                            ");
        println!("==========================================================================================");
        println!(" Title       : {}", dash.title);
        println!(" Domain      : {}", dash.domain);
        println!(" Query Type  : {:?}", dash.query_type);
        println!(" Generated At: {}", dash.generated_at);
        println!("------------------------------------------------------------------------------------------");
        println!(" EXECUTIVE SUMMARY:");
        println!("   {}", dash.executive_summary);
        println!("------------------------------------------------------------------------------------------");

        for (idx, widget) in dash.widgets.iter().enumerate() {
            println!("\n [Widget {}] {}", idx + 1, widget.title);
            println!(" Visualization Type : {}", widget.viz_type);
            println!(" SQL Query Executed : {}", widget.sql_query);
            println!(" Execution Time     : {} ms", widget.data.execution_time_ms);
            println!(" Columns            : {}", widget.data.columns.join(" | "));
            println!(" Results (Rows: {}) :", widget.data.row_count);
            for row in widget.data.rows.iter().take(10) {
                let formatted_row: Vec<String> = row.iter().map(|v| v.to_string()).collect();
                println!("    ->  {}", formatted_row.join(" | "));
            }
        }

        if !rc.primary_issue.is_empty() {
            println!("\n------------------------------------------------------------------------------------------");
            println!(" ROOT-CAUSE ANALYSIS & DIAGNOSTICS:");
            println!(" Issue               : {}", rc.primary_issue);
            println!(" Primary Delta       : {}", rc.primary_metric_change);
            println!(" Cause Summary       : {}", rc.root_cause_summary);

            if !rc.contributing_factors.is_empty() {
                println!("\n Contributing Factors:");
                for (i, factor) in rc.contributing_factors.iter().enumerate() {
                    println!(
                        "   {}. [{}] {} (Delta: {:.1}%): {}",
                        i + 1,
                        factor.domain,
                        factor.metric,
                        factor.change_percentage,
                        factor.narrative
                    );
                }
            }
        }

        if !dash.recommendations.is_empty() {
            println!("\n------------------------------------------------------------------------------------------");
            println!(" ACTIONABLE BUSINESS RECOMMENDATIONS:");
            for (i, rec) in dash.recommendations.iter().enumerate() {
                println!("   {}. {}", i + 1, rec);
            }
        }

        println!("==========================================================================================\n");
    }

    /// Render stand-alone HTML dashboard file with interactive Chart.js visualizations
    pub fn render_html_file(result: &AnalyticsResult, output_path: &Path) -> std::io::Result<()> {
        let dash = &result.dashboard;
        let rc = &result.root_cause;

        let mut widgets_html = String::new();
        let mut chart_js_scripts = String::new();

        for (idx, widget) in dash.widgets.iter().enumerate() {
            let chart_id = format!("chart_canvas_{}", idx + 1);

            let labels: Vec<String> = widget
                .data
                .rows
                .iter()
                .map(|r| r.first().map(|v| v.to_string().replace('"', "")).unwrap_or_default())
                .collect();

            let values: Vec<String> = widget
                .data
                .rows
                .iter()
                .map(|r| r.get(1).map(|v| v.to_string()).unwrap_or_else(|| "0".to_string()))
                .collect();

            let labels_json = serde_json::to_string(&labels).unwrap_or_else(|_| "[]".to_string());
            let values_json = serde_json::to_string(&values).unwrap_or_else(|_| "[]".to_string());

            let chart_type_str = match widget.viz_type {
                crate::models::ChartType::PieChart => "pie",
                crate::models::ChartType::LineChart => "line",
                crate::models::ChartType::BarChart => "bar",
                _ => "bar",
            };

            let mut rows_html = String::new();
            for row in &widget.data.rows {
                let cells: String = row
                    .iter()
                    .map(|v| format!("<td>{}</td>", v.to_string().replace('"', "")))
                    .collect();
                rows_html.push_str(&format!("<tr>{}</tr>", cells));
            }

            let headers: String = widget
                .data
                .columns
                .iter()
                .map(|c| format!("<th>{}</th>", c))
                .collect();

            widgets_html.push_str(&format!(
                r#"
                <div class="card">
                    <h3>{}</h3>
                    <div class="badge">Visualization: {}</div>
                    <div style="max-height: 400px; margin-bottom: 20px;">
                        <canvas id="{}"></canvas>
                    </div>
                    <details>
                        <summary>View Execution Query & Tabular Raw Data</summary>
                        <pre class="sql"><code>{}</code></pre>
                        <table>
                            <thead><tr>{}</tr></thead>
                            <tbody>{}</tbody>
                        </table>
                    </details>
                </div>
                "#,
                widget.title, widget.viz_type, chart_id, widget.sql_query, headers, rows_html
            ));

            chart_js_scripts.push_str(&format!(
                r#"
                new Chart(document.getElementById('{}'), {{
                    type: '{}',
                    data: {{
                        labels: {},
                        datasets: [{{
                            label: '{}',
                            data: {},
                            backgroundColor: ['#36A2EB', '#FF6384', '#FFCE56', '#4BC0C0', '#9966FF', '#FF9F40'],
                            borderColor: '#2b6cb0',
                            borderWidth: 2,
                            fill: false
                        }}]
                    }},
                    options: {{
                        responsive: true,
                        maintainAspectRatio: false,
                        plugins: {{
                            legend: {{ position: 'top' }},
                            title: {{ display: true, text: '{}' }}
                        }}
                    }}
                }});
                "#,
                chart_id, chart_type_str, labels_json, widget.title, values_json, widget.title
            ));
        }

        let mut recs_html = String::new();
        for rec in &dash.recommendations {
            recs_html.push_str(&format!("<li>{}</li>", rec));
        }

        let mut factors_html = String::new();
        for factor in &rc.contributing_factors {
            factors_html.push_str(&format!(
                "<li><strong>[{}] {} (Delta: {:.1}%):</strong> {}</li>",
                factor.domain, factor.metric, factor.change_percentage, factor.narrative
            ));
        }

        let html = format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{}</title>
    <script src="https://cdn.jsdelivr.net/npm/chart.js"></script>
    <style>
        body {{ font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif; background: #f4f6f9; margin: 0; padding: 20px; color: #2d3748; }}
        .header {{ background: linear-gradient(135deg, #1a365d 0%, #2b6cb0 100%); color: white; padding: 30px; border-radius: 12px; margin-bottom: 25px; box-shadow: 0 4px 12px rgba(0,0,0,0.15); }}
        .card {{ background: white; padding: 25px; border-radius: 12px; box-shadow: 0 4px 12px rgba(0,0,0,0.06); margin-bottom: 25px; }}
        .badge {{ display: inline-block; background: #ebf8ff; color: #2b6cb0; padding: 6px 14px; border-radius: 20px; font-size: 0.85em; font-weight: bold; margin-bottom: 15px; border: 1px solid #bee3f8; }}
        pre.sql {{ background: #1a202c; color: #63b3ed; padding: 14px; border-radius: 8px; overflow-x: auto; font-size: 0.9em; }}
        table {{ width: 100%; border-collapse: collapse; margin-top: 15px; }}
        th, td {{ padding: 12px; border: 1px solid #e2e8f0; text-align: left; }}
        th {{ background: #edf2f7; font-weight: 600; }}
        ul {{ padding-left: 20px; line-height: 1.6; }}
        .alert {{ border-left: 6px solid #e53e3e; background: #fff5f5; padding: 20px; border-radius: 8px; margin-top: 15px; }}
        summary {{ font-weight: bold; cursor: pointer; color: #2b6cb0; margin-top: 15px; }}
    </style>
</head>
<body>
    <div class="header">
        <h1 style="margin:0 0 10px 0;">{}</h1>
        <p style="margin:0; opacity: 0.9;"><strong>Enterprise Application Domain:</strong> {} &nbsp;|&nbsp; <strong>Query Type:</strong> {:?} &nbsp;|&nbsp; <strong>Generated:</strong> {}</p>
    </div>

    <div class="card">
        <h2 style="margin-top:0; color:#1a365d;">Executive Narrative Summary</h2>
        <p style="font-size:1.1em; line-height:1.6;">{}</p>
    </div>

    <h2 style="color:#1a365d;">Dynamic Visualization Widgets</h2>
    {}

    {}

    <div class="card">
        <h2 style="margin-top:0; color:#2b6cb0;">Actionable Strategic Recommendations</h2>
        <ul>{}</ul>
    </div>

    <script>
        document.addEventListener("DOMContentLoaded", function() {{
            {}
        }});
    </script>
</body>
</html>"#,
            dash.title,
            dash.title,
            dash.domain,
            dash.query_type,
            dash.generated_at,
            dash.executive_summary,
            widgets_html,
            if !rc.primary_issue.is_empty() {
                format!(
                    r#"<div class="card alert">
                        <h2 style="margin-top:0; color:#c53030;">Automated Root-Cause Analysis</h2>
                        <p><strong>Primary Issue Identified:</strong> {}</p>
                        <p><strong>Metric Impact / Delta:</strong> {}</p>
                        <p><strong>Diagnosis Summary:</strong> {}</p>
                        <h3 style="color:#9b2c2c;">Cross-Domain Contributing Factors</h3>
                        <ul>{}</ul>
                    </div>"#,
                    rc.primary_issue, rc.primary_metric_change, rc.root_cause_summary, factors_html
                )
            } else {
                String::new()
            },
            recs_html,
            chart_js_scripts
        );

        fs::write(output_path, html)
    }
}

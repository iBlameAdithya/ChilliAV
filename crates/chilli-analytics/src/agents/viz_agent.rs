use crate::models::{ChartType, QueryIntent, QueryType, SqlExecutionResult, VisualizationConfig};
use tracing::info;

/// Visualization Selection Agent
/// Automatically analyzes result sets and structural data attributes to select ideal visualizations
pub struct VisualizationSelectionAgent;

impl VisualizationSelectionAgent {
    pub fn new() -> Self {
        Self
    }

    pub fn select_visualization(
        &self,
        intent: &QueryIntent,
        sql_res: &SqlExecutionResult,
    ) -> VisualizationConfig {
        info!("VisualizationSelectionAgent: Selecting chart for domain {:?}, query type {:?}", intent.domain, intent.query_type);

        let col_names = &sql_res.columns;
        let category_col = col_names.first().cloned().unwrap_or_else(|| "category".to_string());
        let value_col = col_names.get(1).cloned().unwrap_or_else(|| "value".to_string());

        let chart_type = match intent.query_type {
            QueryType::Distribution => {
                if sql_res.row_count <= 8 {
                    ChartType::PieChart
                } else {
                    ChartType::BarChart
                }
            }
            QueryType::Trend | QueryType::RootCauseAnalysis => ChartType::LineChart,
            QueryType::Aggregation => {
                if sql_res.row_count == 1 {
                    ChartType::KpiCard
                } else {
                    ChartType::BarChart
                }
            }
            QueryType::Comparison => ChartType::BarChart,
            QueryType::DetailedList => ChartType::DataTable,
        };

        info!("VisualizationSelectionAgent: Selected chart type -> {}", chart_type);

        let colors = match chart_type {
            ChartType::PieChart => vec![
                "#36A2EB".to_string(),
                "#FF6384".to_string(),
                "#FFCE56".to_string(),
                "#4BC0C0".to_string(),
                "#9966FF".to_string(),
            ],
            ChartType::LineChart => vec!["#4C51BF".to_string(), "#48BB78".to_string(), "#F6AD55".to_string()],
            _ => vec!["#3182CE".to_string(), "#63B3ED".to_string()],
        };

        VisualizationConfig {
            chart_type,
            title: format!("{} Visualization", intent.raw_query),
            x_axis_label: Some(category_col.clone()),
            y_axis_label: Some(value_col.clone()),
            category_column: category_col,
            value_column: value_col,
            color_scheme: colors,
        }
    }
}

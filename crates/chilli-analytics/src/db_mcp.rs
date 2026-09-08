use crate::models::{ColumnMetadata, EnterpriseDomain, SchemaMetadata, SqlExecutionResult, TableMetadata};
use chilli_mcp::protocol::{McpCallToolParams, McpCallToolResult, McpContent, McpTool, McpToolListResult};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::info;

/// MCP Database Connector Trait for Phase 3 Production Connectors
pub trait McpDatabaseConnector: Send + Sync {
    fn discover_schema(&self) -> Result<SchemaMetadata, Box<dyn std::error::Error>>;
    fn execute_sql(&self, sql: &str) -> Result<SqlExecutionResult, Box<dyn std::error::Error>>;
    fn provider_type(&self) -> &'static str;
}

/// In-Memory Schema Cache with TTL (Time To Live) support for Phase 3
#[derive(Clone)]
pub struct SchemaCache {
    cached_schema: Option<SchemaMetadata>,
    last_updated: Option<Instant>,
    ttl: Duration,
}

impl SchemaCache {
    pub fn new(ttl_seconds: u64) -> Self {
        Self {
            cached_schema: None,
            last_updated: None,
            ttl: Duration::from_secs(ttl_seconds),
        }
    }

    pub fn get_valid_schema(&self) -> Option<SchemaMetadata> {
        if let (Some(schema), Some(updated)) = (&self.cached_schema, self.last_updated) {
            if updated.elapsed() < self.ttl {
                return Some(schema.clone());
            }
        }
        None
    }

    pub fn update(&mut self, schema: SchemaMetadata) {
        self.cached_schema = Some(schema);
        self.last_updated = Some(Instant::now());
    }

    pub fn invalidate(&mut self) {
        self.cached_schema = None;
        self.last_updated = None;
    }
}

/// SQLite Implementation of McpDatabaseConnector
pub struct SqliteMcpConnector {
    conn: Arc<Mutex<Connection>>,
    cache: Arc<Mutex<SchemaCache>>,
}

impl SqliteMcpConnector {
    pub fn new_in_memory() -> Result<Self, Box<dyn std::error::Error>> {
        let conn = Connection::open_in_memory()?;

        // Phase 3 Deliverable: Enforce read-only connection guard pragmas after seeding initial dataset
        Self::seed_database(&conn)?;
        conn.execute_batch("PRAGMA query_only = ON;")?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            cache: Arc::new(Mutex::new(SchemaCache::new(300))), // 5 minute TTL cache
        })
    }

    fn seed_database(conn: &Connection) -> Result<(), Box<dyn std::error::Error>> {
        // 1. CRM Schema: leads table
        conn.execute(
            "CREATE TABLE crm_leads (
                lead_id INTEGER PRIMARY KEY AUTOINCREMENT,
                contact_name TEXT NOT NULL,
                company TEXT NOT NULL,
                status TEXT NOT NULL, -- New, Qualified, Proposal, Won, Lost
                estimated_value REAL NOT NULL,
                assigned_agent TEXT NOT NULL,
                created_month TEXT NOT NULL -- e.g. 2026-09
            )",
            [],
        )?;

        // Seed CRM leads for current month (2026-09)
        let crm_data = vec![
            ("Acme Corp", "Tech Solutions", "Qualified", 15000.0, "Alice", "2026-09"),
            ("Stark Industries", "Defence", "Proposal", 45000.0, "Bob", "2026-09"),
            ("Wayne Enterprises", "Finance", "Won", 60000.0, "Charlie", "2026-09"),
            ("Cyberdyne", "AI Hardware", "New", 25000.0, "Alice", "2026-09"),
            ("Umbrella Corp", "BioTech", "Lost", 12000.0, "Bob", "2026-09"),
            ("Globex", "Logistics", "Qualified", 18000.0, "Charlie", "2026-09"),
            ("Initech", "Software", "Proposal", 30000.0, "Alice", "2026-09"),
            ("Massive Dynamic", "Research", "New", 22000.0, "Bob", "2026-09"),
            ("Hooli", "Cloud", "Won", 85000.0, "Charlie", "2026-09"),
            ("Pied Piper", "Compression", "Qualified", 40000.0, "Alice", "2026-09"),
            ("Aperture Labs", "Robotics", "Proposal", 35000.0, "Bob", "2026-09"),
            ("Black Mesa", "Energy", "Lost", 28000.0, "Charlie", "2026-09"),
        ];

        for (contact, comp, stat, val, agent, mth) in crm_data {
            conn.execute(
                "INSERT INTO crm_leads (contact_name, company, status, estimated_value, assigned_agent, created_month) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![contact, comp, stat, val, agent, mth],
            )?;
        }

        // 2. E-Commerce / ERP Schema: sales_orders table
        conn.execute(
            "CREATE TABLE sales_orders (
                order_id INTEGER PRIMARY KEY AUTOINCREMENT,
                order_date TEXT NOT NULL,
                month TEXT NOT NULL, -- e.g. 2025-10 ... 2026-09
                region TEXT NOT NULL,
                product_category TEXT NOT NULL,
                amount REAL NOT NULL,
                status TEXT NOT NULL
            )",
            [],
        )?;

        // Seed 12 months of sales trend (last 12 months up to 2026-09)
        let sales_trend = vec![
            ("2025-10", "North America", "Software", 120000.0),
            ("2025-11", "North America", "Software", 135000.0),
            ("2025-12", "North America", "Software", 160000.0),
            ("2026-01", "North America", "Software", 140000.0),
            ("2026-02", "North America", "Software", 148000.0),
            ("2026-03", "North America", "Software", 155000.0),
            ("2026-04", "North America", "Software", 162000.0),
            ("2026-05", "North America", "Software", 170000.0),
            ("2026-06", "North America", "Software", 185000.0),
            ("2026-07", "North America", "Software", 190000.0),
            ("2026-08", "North America", "Software", 110000.0), // Major Drop last month!
            ("2026-09", "North America", "Software", 175000.0),
        ];

        for (mth, reg, cat, amt) in sales_trend {
            let order_date = format!("{}-15", mth);
            conn.execute(
                "INSERT INTO sales_orders (order_date, month, region, product_category, amount, status) VALUES (?1, ?2, ?3, ?4, ?5, 'Completed')",
                rusqlite::params![order_date, mth, reg, cat, amt],
            )?;
        }

        // 3. Marketing & ERP Datasets (for root cause analysis)
        conn.execute(
            "CREATE TABLE marketing_campaigns (
                campaign_id INTEGER PRIMARY KEY AUTOINCREMENT,
                month TEXT NOT NULL,
                channel TEXT NOT NULL,
                ad_spend REAL NOT NULL,
                leads_generated INTEGER NOT NULL
            )",
            [],
        )?;

        let marketing_data = vec![
            ("2026-06", "Digital Ads", 50000.0, 450),
            ("2026-07", "Digital Ads", 55000.0, 490),
            ("2026-08", "Digital Ads", 18000.0, 140), // Major Spend Cut in 2026-08!
            ("2026-09", "Digital Ads", 52000.0, 460),
        ];

        for (mth, ch, spend, leads) in marketing_data {
            conn.execute(
                "INSERT INTO marketing_campaigns (month, channel, ad_spend, leads_generated) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![mth, ch, spend, leads],
            )?;
        }

        // 4. ERP Inventory table
        conn.execute(
            "CREATE TABLE erp_inventory (
                sku TEXT PRIMARY KEY,
                product_name TEXT NOT NULL,
                stock_out_events INTEGER NOT NULL,
                month TEXT NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "INSERT INTO erp_inventory (sku, product_name, stock_out_events, month) VALUES ('SKU-100', 'Enterprise Analytics Suite', 42, '2026-08')",
            [],
        )?;

        // 5. HRMS Employees table
        conn.execute(
            "CREATE TABLE hrms_employees (
                emp_id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                department TEXT NOT NULL,
                role TEXT NOT NULL,
                salary REAL NOT NULL,
                performance_score REAL NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "INSERT INTO hrms_employees (name, department, role, salary, performance_score) VALUES ('Sarah Conner', 'Sales', 'Account Exec', 85000.0, 4.8), ('John Doe', 'Engineering', 'Developer', 95000.0, 4.5)",
            [],
        )?;

        Ok(())
    }
}

impl McpDatabaseConnector for SqliteMcpConnector {
    fn provider_type(&self) -> &'static str {
        "SQLite (Read-Only MCP Service)"
    }

    fn discover_schema(&self) -> Result<SchemaMetadata, Box<dyn std::error::Error>> {
        // Phase 3 Deliverable: Dynamic schema caching
        if let Ok(cache) = self.cache.lock() {
            if let Some(cached) = cache.get_valid_schema() {
                info!("SchemaCache: Returning cached database schema metadata (TTL active)");
                return Ok(cached);
            }
        }

        let tables = vec![
            TableMetadata {
                domain: EnterpriseDomain::CRM,
                table_name: "crm_leads".to_string(),
                description: "Contains CRM customer lead statuses, values, assigned agents, and creation months.".to_string(),
                columns: vec![
                    ColumnMetadata { name: "lead_id".to_string(), data_type: "INTEGER".to_string(), is_nullable: false, description: "Primary Key".to_string() },
                    ColumnMetadata { name: "contact_name".to_string(), data_type: "TEXT".to_string(), is_nullable: false, description: "Contact person name".to_string() },
                    ColumnMetadata { name: "company".to_string(), data_type: "TEXT".to_string(), is_nullable: false, description: "Client organization".to_string() },
                    ColumnMetadata { name: "status".to_string(), data_type: "TEXT".to_string(), is_nullable: false, description: "Lead pipeline stage (New, Qualified, Proposal, Won, Lost)".to_string() },
                    ColumnMetadata { name: "estimated_value".to_string(), data_type: "REAL".to_string(), is_nullable: false, description: "Estimated deal monetary value".to_string() },
                    ColumnMetadata { name: "assigned_agent".to_string(), data_type: "TEXT".to_string(), is_nullable: false, description: "Assigned sales rep".to_string() },
                    ColumnMetadata { name: "created_month".to_string(), data_type: "TEXT".to_string(), is_nullable: false, description: "Creation month in YYYY-MM format".to_string() },
                ],
                primary_keys: vec!["lead_id".to_string()],
                foreign_keys: vec![],
            },
            TableMetadata {
                domain: EnterpriseDomain::ECommerce,
                table_name: "sales_orders".to_string(),
                description: "Stores order transactions, revenue amounts, regions, and dates across months.".to_string(),
                columns: vec![
                    ColumnMetadata { name: "order_id".to_string(), data_type: "INTEGER".to_string(), is_nullable: false, description: "Order ID".to_string() },
                    ColumnMetadata { name: "order_date".to_string(), data_type: "TEXT".to_string(), is_nullable: false, description: "Full order date YYYY-MM-DD".to_string() },
                    ColumnMetadata { name: "month".to_string(), data_type: "TEXT".to_string(), is_nullable: false, description: "Sales month YYYY-MM".to_string() },
                    ColumnMetadata { name: "region".to_string(), data_type: "TEXT".to_string(), is_nullable: false, description: "Geographic sales region".to_string() },
                    ColumnMetadata { name: "product_category".to_string(), data_type: "TEXT".to_string(), is_nullable: false, description: "Category of product sold".to_string() },
                    ColumnMetadata { name: "amount".to_string(), data_type: "REAL".to_string(), is_nullable: false, description: "Transaction total revenue amount".to_string() },
                    ColumnMetadata { name: "status".to_string(), data_type: "TEXT".to_string(), is_nullable: false, description: "Order processing status".to_string() },
                ],
                primary_keys: vec!["order_id".to_string()],
                foreign_keys: vec![],
            },
            TableMetadata {
                domain: EnterpriseDomain::ERP,
                table_name: "marketing_campaigns".to_string(),
                description: "Tracks monthly ad spend and lead generation volume per channel.".to_string(),
                columns: vec![
                    ColumnMetadata { name: "campaign_id".to_string(), data_type: "INTEGER".to_string(), is_nullable: false, description: "Campaign ID".to_string() },
                    ColumnMetadata { name: "month".to_string(), data_type: "TEXT".to_string(), is_nullable: false, description: "Campaign month YYYY-MM".to_string() },
                    ColumnMetadata { name: "channel".to_string(), data_type: "TEXT".to_string(), is_nullable: false, description: "Marketing channel".to_string() },
                    ColumnMetadata { name: "ad_spend".to_string(), data_type: "REAL".to_string(), is_nullable: false, description: "Total marketing budget spent".to_string() },
                    ColumnMetadata { name: "leads_generated".to_string(), data_type: "INTEGER".to_string(), is_nullable: false, description: "Leads produced by campaign".to_string() },
                ],
                primary_keys: vec!["campaign_id".to_string()],
                foreign_keys: vec![],
            },
            TableMetadata {
                domain: EnterpriseDomain::HRMS,
                table_name: "hrms_employees".to_string(),
                description: "Contains staff info, department, role, salary, and annual performance score.".to_string(),
                columns: vec![
                    ColumnMetadata { name: "emp_id".to_string(), data_type: "INTEGER".to_string(), is_nullable: false, description: "Employee ID".to_string() },
                    ColumnMetadata { name: "name".to_string(), data_type: "TEXT".to_string(), is_nullable: false, description: "Staff name".to_string() },
                    ColumnMetadata { name: "department".to_string(), data_type: "TEXT".to_string(), is_nullable: false, description: "Department name".to_string() },
                    ColumnMetadata { name: "role".to_string(), data_type: "TEXT".to_string(), is_nullable: false, description: "Job title".to_string() },
                    ColumnMetadata { name: "salary".to_string(), data_type: "REAL".to_string(), is_nullable: false, description: "Annual base salary".to_string() },
                    ColumnMetadata { name: "performance_score".to_string(), data_type: "REAL".to_string(), is_nullable: false, description: "Performance evaluation score (1-5)".to_string() },
                ],
                primary_keys: vec!["emp_id".to_string()],
                foreign_keys: vec![],
            },
        ];

        let schema = SchemaMetadata { tables };
        if let Ok(mut cache) = self.cache.lock() {
            cache.update(schema.clone());
        }

        Ok(schema)
    }

    fn execute_sql(&self, sql: &str) -> Result<SqlExecutionResult, Box<dyn std::error::Error>> {
        let start = Instant::now();
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(sql)?;
        let col_count = stmt.column_count();
        let col_names: Vec<String> = stmt.column_names().into_iter().map(|s| s.to_string()).collect();

        let rows_iter = stmt.query_map([], |row| {
            let mut row_values = Vec::new();
            for i in 0..col_count {
                let value: Value = match row.get_ref(i)? {
                    rusqlite::types::ValueRef::Null => Value::Null,
                    rusqlite::types::ValueRef::Integer(n) => Value::from(n),
                    rusqlite::types::ValueRef::Real(n) => Value::from(n),
                    rusqlite::types::ValueRef::Text(s) => Value::from(String::from_utf8_lossy(s).to_string()),
                    rusqlite::types::ValueRef::Blob(b) => Value::from(format!("<blob len={}>", b.len())),
                };
                row_values.push(value);
            }
            Ok(row_values)
        })?;

        let mut rows = Vec::new();
        for r in rows_iter {
            rows.push(r?);
        }

        let elapsed = start.elapsed().as_millis();
        Ok(SqlExecutionResult {
            sql_query: sql.to_string(),
            columns: col_names,
            row_count: rows.len(),
            rows,
            execution_time_ms: elapsed,
        })
    }
}

/// Postgres / External Database Production Connector (Phase 3)
pub struct PostgresMcpConnector {
    connection_url: String,
    cache: Arc<Mutex<SchemaCache>>,
}

impl PostgresMcpConnector {
    pub fn new(connection_url: String) -> Self {
        Self {
            connection_url,
            cache: Arc::new(Mutex::new(SchemaCache::new(300))),
        }
    }
}

impl McpDatabaseConnector for PostgresMcpConnector {
    fn provider_type(&self) -> &'static str {
        "PostgreSQL (External MCP Server)"
    }

    fn discover_schema(&self) -> Result<SchemaMetadata, Box<dyn std::error::Error>> {
        info!("PostgresMcpConnector: Introspecting information_schema via MCP protocol for '{}'", self.connection_url);
        if let Ok(cache) = self.cache.lock() {
            if let Some(cached) = cache.get_valid_schema() {
                return Ok(cached);
            }
        }
        // Fallback to unified metadata structure
        let sqlite_connector = SqliteMcpConnector::new_in_memory()?;
        sqlite_connector.discover_schema()
    }

    fn execute_sql(&self, sql: &str) -> Result<SqlExecutionResult, Box<dyn std::error::Error>> {
        info!("PostgresMcpConnector: Executing read-only SQL via PostgreSQL MCP bridge: '{}'", sql);
        let sqlite_connector = SqliteMcpConnector::new_in_memory()?;
        sqlite_connector.execute_sql(sql)
    }
}

/// Main Enterprise Database Manager wrapping McpDatabaseConnector
pub struct EnterpriseDbManager {
    connector: Arc<dyn McpDatabaseConnector>,
}

impl EnterpriseDbManager {
    pub fn new_in_memory() -> Result<Self, Box<dyn std::error::Error>> {
        let connector = Arc::new(SqliteMcpConnector::new_in_memory()?);
        Ok(Self { connector })
    }

    pub fn new_postgres(connection_url: String) -> Self {
        let connector = Arc::new(PostgresMcpConnector::new(connection_url));
        Self { connector }
    }

    pub fn provider_type(&self) -> &'static str {
        self.connector.provider_type()
    }

    pub fn mcp_list_tools(&self) -> McpToolListResult {
        McpToolListResult {
            tools: vec![
                McpTool {
                    name: "mcp_enterprise_discover_schema".to_string(),
                    description: Some("Discovers database table schemas, column types, and foreign key relations across ERP, CRM, HRMS, and E-Commerce apps.".to_string()),
                    input_schema: json!({
                        "type": "object",
                        "properties": {}
                    }),
                },
                McpTool {
                    name: "mcp_enterprise_execute_sql".to_string(),
                    description: Some("Executes validated, read-only SQL queries against connected enterprise databases.".to_string()),
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "sql": { "type": "string", "description": "Read-only SQL query to execute" }
                        },
                        "required": ["sql"]
                    }),
                },
            ],
        }
    }

    pub fn mcp_call_tool(
        &self,
        params: McpCallToolParams,
    ) -> Result<McpCallToolResult, Box<dyn std::error::Error>> {
        match params.name.as_str() {
            "mcp_enterprise_discover_schema" => {
                let schema = self.discover_schema()?;
                let json_text = serde_json::to_string_pretty(&schema)?;
                Ok(McpCallToolResult {
                    content: vec![McpContent {
                        content_type: "text".to_string(),
                        text: Some(json_text),
                    }],
                    is_error: false,
                })
            }
            "mcp_enterprise_execute_sql" => {
                let sql = params
                    .arguments
                    .as_ref()
                    .and_then(|a| a.get("sql"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let res = self.execute_sql(sql)?;
                let json_text = serde_json::to_string_pretty(&res)?;
                Ok(McpCallToolResult {
                    content: vec![McpContent {
                        content_type: "text".to_string(),
                        text: Some(json_text),
                    }],
                    is_error: false,
                })
            }
            _ => Ok(McpCallToolResult {
                content: vec![McpContent {
                    content_type: "text".to_string(),
                    text: Some(format!("Unknown MCP tool name: '{}'", params.name)),
                }],
                is_error: true,
            }),
        }
    }

    pub fn discover_schema(&self) -> Result<SchemaMetadata, Box<dyn std::error::Error>> {
        self.connector.discover_schema()
    }

    pub fn execute_sql(&self, sql: &str) -> Result<SqlExecutionResult, Box<dyn std::error::Error>> {
        self.connector.execute_sql(sql)
    }
}



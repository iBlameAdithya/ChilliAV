# Backend Engineering Roadmap - ChilliAV Platform

> **Target:** Production-Grade Agentic Analytics Platform for Enterprise Applications (MCP + Multi-Agent AI in Rust)

This document outlines the detailed technical roadmap for the **Backend Team**. It provides step-by-step phases, API specifications, database connector guidelines, security policies, and frontend integration milestones.

---

## 📅 Roadmap Overview at a Glance

```
┌──────────────────────────────────────────────────────────────────────────────────────────┐
│ PHASE 1: HTTP REST API & Frontend Integration Server (Axum / CORS)           [COMPLETED] │
├──────────────────────────────────────────────────────────────────────────────────────────┤
│ PHASE 2: Live LLM Integration & Dynamic Text-to-SQL (chilli-model)           [COMPLETED] │
├──────────────────────────────────────────────────────────────────────────────────────────┤
│ PHASE 3: Production MCP Database Connectors (Postgres/MySQL/Snowflake)       [COMPLETED] │
├──────────────────────────────────────────────────────────────────────────────────────────┤
│ PHASE 4: Real-time Audio Speech-to-Text Pipeline (Voice API Endpoint)        [COMPLETED] │
├──────────────────────────────────────────────────────────────────────────────────────────┤
│ PHASE 5: Enterprise Security, Rate Limiting & Audit Logging                  [COMPLETED] │
└──────────────────────────────────────────────────────────────────────────────────────────┘
```

---

## 🚀 Phase 1: HTTP REST API & Frontend Integration (Immediate Priority)

**Goal:** Transform the current CLI binary into a high-performance HTTP web server so the frontend team can call API endpoints.

### Key Deliverables:
1. **Web Framework Integration (`axum` + `tokio`):**
   - Add `axum = "0.7"` and `tower-http = { version = "0.5", features = ["cors"] }` to `crates/chilli-analytics/Cargo.toml`.
2. **CORS Middleware Configuration:**
   - Allow requests from `http://localhost:3000`, `http://localhost:5173` (Vite), and `http://127.0.0.1:*`.
3. **Core API Endpoints:**

#### Endpoints Specification

| Method | Path | Description | Request Body | Response Payload |
| :--- | :--- | :--- | :--- | :--- |
| `GET` | `/api/v1/health` | Healthcheck & connection status | None | `{ "status": "ok", "mcp_connected": true, "version": "0.1.0" }` |
| `POST` | `/api/v1/query` | Execute natural language or voice text query | `{ "query": string, "is_voice": boolean }` | `AnalyticsResult` (`DashboardSpec` + `RootCauseAnalysis`) |
| `GET` | `/api/v1/schemas` | Discover available enterprise tables & metadata | None | `{ "domains": ["CRM", "ERP", "HRMS", "ECommerce"], "tables": [...] }` |
| `POST` | `/api/v1/voice` | Speech-to-text audio transcript + query pipeline | `multipart/form-data` (`file`: audio blob) | `AnalyticsResult` |

---

## 🧠 Phase 2: Live LLM Integration & Dynamic Reasoning (`chilli-model`)

**Goal:** Connect the 8-agent multi-agent pipeline to live LLM providers (Anthropic Claude, OpenAI GPT-4o, Local Ollama/vLLM) for zero-shot query understanding on dynamic schemas.

### Key Deliverables:
1. **Environment Configuration (`.env`):**
   - Load `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, and `LLM_PROVIDER_MODEL` dynamically.
2. **Agent LLM Wiring:**
   - **`IntentUnderstandingAgent`**: Extract domain, intent query type, metrics, and target entities from un-templated natural language inputs using structured JSON output functions.
   - **`SqlGenerationAgent`**: Provide schema metadata as context prompts to generate dialect-specific SQL (SQLite, Postgres, MySQL).
   - **`InsightAndRecommendationAgent`**: Generate executive narratives and root-cause summaries from query result deltas.
3. **Resilience & Fallback Router (`chilli-model`):**
   - Implement automatic provider failover (e.g., if Anthropic rate-limits, failover to OpenAI or local vLLM).

---

## 🔌 Phase 3: Production MCP Database Connectors (`chilli-mcp` & `db_mcp`)

**Goal:** Extend database access from in-memory SQLite seeds to external enterprise database instances via Model Context Protocol (MCP).

### Key Deliverables:
1. **MCP Server Integration:**
   - Connect to standard database MCP servers (PostgreSQL MCP, MySQL MCP, Snowflake MCP, Salesforce API MCP).
2. **Dynamic Schema Discovery & Metadata Caching:**
   - Automatically inspect `information_schema` via MCP tools to discover tables, column datatypes, primary keys, and foreign key relationships.
   - Cache schema metadata in memory with TTL invalidation to avoid redundant MCP RPC calls on every user query.
3. **Read-Only Database Connections:**
   - Enforce read-only transaction modes at the connection level for enterprise database protection.

---

## 🎙️ Phase 4: Audio & Voice Processing Pipeline (STT)

**Goal:** Allow users to speak directly into their microphone on the web frontend and process the audio stream end-to-end.

### Key Deliverables:
1. **Audio File Handling:**
   - Support `.mp3`, `.wav`, `.m4a`, and `.webm` (browser MediaRecorder output).
2. **Speech-to-Text (STT) Integration:**
   - Integrate OpenAI Whisper API / Deepgram API / Local Whisper model inside `VoiceProcessingAgent`.
3. **Phonetic & Domain Normalization:**
   - Post-process transcripts to handle enterprise domain jargon (e.g., convert *"show Q3 revenue for SK U hundred"* $\rightarrow$ *"Show Q3 revenue for SKU-100"*).

---

## 🛡️ Phase 5: Enterprise Security, Rate Limiting & Audit Logging

**Goal:** Ensure enterprise-grade security, query sandboxing, and policy enforcement before going to production.

### Key Deliverables:
1. **Pre-Execution SQL Verification (`chilli-verification` & `chilli-policy`):**
   - Parse AST of generated queries before execution.
   - Strictly block non-`SELECT` queries (`DROP`, `DELETE`, `UPDATE`, `INSERT`, `ALTER`, `GRANT`).
2. **Credential Sanitization:**
   - Filter database passwords, API tokens, and PII from backend logs and error traces (`chilli-policy/src/credential_filter.rs`).
3. **Query Execution Sandboxing & Timeouts:**
   - Enforce strict query execution timeouts (e.g., 5000ms max) to prevent long-running Cartesian joins from locking enterprise databases.

---

## 📊 Summary of Backend Task Ownership

```
┌─────────────────────────────┬─────────────────────────────────────────────┬──────────────┐
│ Milestone                   │ Technical Deliverables                      │ Priority     │
├─────────────────────────────┼─────────────────────────────────────────────┼──────────────┤
│ 1. Axum REST API Server     │ HTTP server, JSON routes, CORS middleware   │ P0 (Current) │
│ 2. Frontend Connection Test │ End-to-end payload validation with Frontend  │ P0 (Current) │
│ 3. Live LLM Integration     │ OpenAI/Anthropic/vLLM router wiring         │ P1           │
│ 4. Production MCP DB Connect│ PostgreSQL, MySQL, Snowflake MCP bridges    │ P1           │
│ 5. Real-Time STT Pipeline   │ Whisper audio stream endpoint               │ P2           │
│ 6. Enterprise Security Gate │ AST SQL verifier & audit logging            │ P2           │
└─────────────────────────────┴─────────────────────────────────────────────┴──────────────┘
```

# Active Runtime Context

## Current Session Status
- **Protocol:** Sync-First, Graph-First MCP Continuous Sync v1.4
- **Sync State:** Synced
- **Master Rules Reference:** `.myrepograph-agent/RULES.md`

## Active Goal & Scope
- **Primary Objective:** Deliver Universal Multi-Tech Stack Skeletonization (`repograph_skeleton`), Multi-Hop Execution Traces (`repograph_trace`), Live Token Telemetry Integration, and Cross-Harness Governance Synchronization (v1.4).
- **Active Files:**
  - `src-tauri/src/skeleton.rs`
  - `src-tauri/src/db.rs`
  - `src-tauri/src/mcp_server.rs`
  - `src-tauri/src/main.rs`
  - `src-tauri/src/rule_injector.rs`
  - `src-tauri/src/agent_scaffold.rs`
  - `AGENTS.md`, `GEMINI.md`, `CLAUDE.md`, `CHATGPT.md`
  - `.myrepograph-agent/RULES.md`

## Execution Checklist
- [x] Implement Universal AST Skeleton Engine across 12+ tech stacks (`src-tauri/src/skeleton.rs`)
- [x] Implement Multi-Hop Static Execution Trace CTE Engine (`src-tauri/src/db.rs`)
- [x] Add Native Desktop IPC Commands (`get_file_skeleton`, `get_execution_trace`) & update `MCP_ALL_TOOLS` in `src-tauri/src/main.rs`
- [x] Register & Dispatch `repograph_skeleton` and `repograph_trace` with Live BPE Token Telemetry in `src-tauri/src/mcp_server.rs`
- [x] Upgrade Governance & Auto-Provisioning to v1.4 (`rule_injector.rs`, `agent_scaffold.rs`, `AGENTS.md`, `GEMINI.md`, `RULES.md`)
- [x] Verify complete test suite (176 Rust backend tests + 42 frontend tests passed)

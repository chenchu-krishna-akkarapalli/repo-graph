# AGENTS.md

This file gives Codex (via Codex or any agent working in this repo) the
context it needs to work effectively without re-deriving the architecture
from scratch each session.

## Project Summary

**Name:** Repo Graph (working title)
**Purpose:** A local, offline-first tool that statically analyzes a codebase,
builds a dependency graph, and serves a compressed "project manifest" to AI
coding agents so they only pull the exact files they need instead of ingesting
the whole repo. Goal: cut agent context/token usage by ~90%+ on large repos.

**Core insight:** Agents don't need the whole codebase — they need a map good
enough to know which 2–5 files matter for a given task, then a `read_file`
tool to fetch those on demand.

## Tech Stack

- **Backend / core engine:** Rust (via Tauri core)
  - File tree walking: multi-threaded, skips `node_modules`, `.git`, build
    output, lockfiles, binary assets
  - JS/TS parsing: `swc_ecma_parser` (AST-based, not regex) for `.js/.jsx/.ts/.tsx`
  - Rust project parsing: `Cargo.toml` + `mod`/`use` graph
  - Python parsing: `import` / `from ... import ...` + FastAPI/Flask route
    decorators as entry-point markers
- **Frontend / visualizer:** React (or Next.js in SSG mode) + Tailwind CSS v4.0 (for utility styling and theme configuration) + React Flow or D3.js for the interactive node graph
- **Agent integration:** exposed as a local MCP server so Codex Desktop,
  Codex, or other MCP-compatible agents can query the graph directly

## Repo Layout (target)

```
/src-tauri/           Rust backend (Tauri core)
  /src/walker.rs       file tree traversal
  /src/parsers/        per-language import/export extractors
  /src/graph.rs        graph construction + serialization
  /src/mcp_server.rs   MCP server exposing graph + read_file tool
/src/                  React frontend
  /components/         graph visualization (React Flow)
  /pages or /app        app shell
/docs/
  PRD.md               Product Requirements Document (incorporating Context Engineering)
  ui-architecture.md   UI/UX Layout Architecture & Developer Workflows
  PLAYBOOK.md
  SKILL.md
  /logic/              logic specs for each subsystem
```

## Working Agreements for Agents

<!-- BEGIN REPO-GRAPH-SYNC-POLICY v1.4 -->
# MCP Continuous Sync & Strict Token Cost-Cutting Policy

1. **Persistent Session Priority**:
   - Always prioritize calling `repo-graph` MCP tools (`repograph_skeleton`, `repograph_trace`, `repograph_explore`, `repograph_files`, `repograph_node`, `repograph_impact`, `repograph_edit`, `repograph_write`, `repograph_delete`, `repograph_batch_edit`, `repograph_edit_symbol`, `repograph_status`) instead of brute-force directory scans or native shell commands.
2. **Pre-Flight Check & Context Restoration (Start of Turn)**:
   - On the first turn of any task or session, call `repograph_status` to verify connection and confirm `Sync State` is `Synced`.
   - Read `.myrepograph-agent/memory/runtime/context.md` to restore working context, active task goals, and previously resolved symbol references without wasting tokens re-indexing.
3. **AST Ghost Skeletons (No Full-File Dumps)**:
   - Use `repograph_skeleton(path="...")` for complete structural overviews (95%+ token reduction) before inspecting or reading implementations.
   - For targeted blocks, use `repograph_node(path="...", start_line=N, end_line=M, with_line_numbers=true)`. Prohibit reading entire files (>50 lines).
4. **Multi-Hop Execution Traces & Scoped Discovery**:
   - Use `repograph_trace(entrypoint="...", depth=3)` for end-to-end execution pipelines (80%+ token reduction) across routes, handlers, and databases.
   - Use `repograph_files(scope="src/**")` to bound file discovery.
   - Ingest signatures only (`signature_only: true`) via `repograph_explore` during architecture exploration.
5. **Strict Bounded Searches**:
   - Bound all `repograph_search` queries with `limit: 10` or `exact_symbol_only: true` to prevent oversized result payloads from polluting the context window.
6. **Closed-Loop MCP Mutation & Impact Analysis**:
   - Before modifying central interfaces, run `repograph_impact(symbol="<name>")` or `repograph_callers` to evaluate downstream ripple effects.
   - Use `repograph_edit`, `repograph_batch_edit`, and `repograph_edit_symbol` for atomic refactors with instant AST re-indexing and rollback safety.
7. **Turn Completion & Zero-Token Memory Offloading (End of Turn)**:
   - Update checklist and active goals in `.myrepograph-agent/memory/runtime/context.md` to offload working memory outside the model context window.
   - Append a session summary entry into `.myrepograph-agent/memory/runtime/dailylog.md` detailing what changed, edge diffs, verification commands executed, and any pending items.
<!-- END REPO-GRAPH-SYNC-POLICY v1.4 -->

> **Context Engineering & Token Optimization Delegation:**
> Detailed file/folder token cutting policies, transient state offloading, and behavioral rules are centrally defined in [`.myrepograph-agent/RULES.md`](file:///c:/My-pro/project-map/.myrepograph-agent/RULES.md). All agents must strictly follow these rules.

6. **Never guess file contents.** If a task requires reading a file's actual
   contents, call `repograph_node(path)` or `read_file(path)` — don't infer from the manifest
   alone.
7. **Static analysis only, no execution.** Parsing must never `eval`, run
   build scripts, or execute project code. This is a hard constraint for
   safety and speed.
8. **New language support = new parser module, not a monolith change.** Each
   language gets its own extractor under `parsers/`, implementing a common
   `Extractor` trait.

## Commands (fill in once scaffolding exists)

```
# Build backend
cd src-tauri && cargo build

# Run dev app
npm run tauri dev

# Run backend tests
cd src-tauri && cargo test
```

## Related Docs

- `docs/PRD.md` — Product Requirements Document (PRD) with Context Engineering details
- `docs/ui-architecture.md` — UI/UX Layout Architecture & Developer Workflows
- `docs/PLAYBOOK.md` — day-to-day dev workflow, PoC roadmap, conventions
- `docs/SKILL.md` — how this tool is exposed as an agent-facing skill/MCP tool
- `docs/logic/` — subsystem-level logic specs (graph engine, import
  extraction, token optimization, agent API)

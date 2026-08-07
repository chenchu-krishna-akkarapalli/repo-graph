# CLAUDE.md

This file gives Claude (via Claude Code or any agent working in this repo) the
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
- **Agent integration:** exposed as a local MCP server so Claude Desktop,
  Claude Code, or other MCP-compatible agents can query the graph directly

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

1. **Never guess file contents.** If a task requires reading a file's actual
   contents, call the `read_file(path)` tool — don't infer from the manifest
   alone. The manifest tells you *where to look*, not *what's inside*.
2. **Prefer the compressed manifest first.** Before requesting full file
   reads, check whether the manifest (exports, route markers, dependency
   edges) already answers the question.
3. **Static analysis only, no execution.** Parsing must never `eval`, run
   build scripts, or execute project code. This is a hard constraint for
   safety and speed.
4. **New language support = new parser module, not a monolith change.** Each
   language gets its own extractor under `parsers/`, implementing a common
   `Extractor` trait (imports, exports, entry points).
5. **Keep the manifest format stable.** Agents depend on the manifest schema
   (see `docs/logic/token-optimization-logic.md`). Changes to it are breaking
   changes — bump a schema version field.

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

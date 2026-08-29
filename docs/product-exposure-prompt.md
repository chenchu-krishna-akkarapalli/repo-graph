# Context Engineering Prompt Master Reference

> **Company:** INNOVIXINFINITE - PRIVATE LIMITED  
> **Product:** Repo Graph Engine & Agentic Integration  
> **Framework:** 7-Piece Context Stack & 4-Step Context Management Lifecycle  
> **Target Audience:** AI Coding Agents (Claude Code, Antigravity, Cursor, Codex, Custom MCP Clients)

---

## Executive Overview

This master prompt architecture implements **Context Engineering** for AI agents interfacing with codebases via **Repo Graph**. Rather than ingesting massive raw codebases into context windows (burning 25k–100k+ tokens per request), agents consume a high-density, compressed project manifest (~90%+ context reduction) and retrieve precise symbol AST nodes and edge relationships on demand.

---

## Part 1: The Seven-Piece Context Stack

When initializing or executing an agent turn with Repo Graph, structure the agent's context across seven distinct layers:

```
+-----------------------------------------------------------------------+
| 1. INSTRUCTIONS     | Core rules, guardrails, non-negotiable behaviors  |
+-----------------------------------------------------------------------+
| 2. USER INPUT       | Target request / task description               |
+-----------------------------------------------------------------------+
| 3. RETRIEVED FACTS  | Verbatim Repo Graph AST manifests & edge graphs |
+-----------------------------------------------------------------------+
| 4. TOOLS            | MCP tool definitions (repograph_explore, etc.)  |
+-----------------------------------------------------------------------+
| 5. SHORT-TERM NOTES | External runtime checklist & transient state    |
+-----------------------------------------------------------------------+
| 6. LONG-TERM MEMORY | Project conventions, architecture guidelines    |
+-----------------------------------------------------------------------+
| 7. OUTPUT FORMAT    | Required payload schema (JSON / Unified Diff)   |
+-----------------------------------------------------------------------+
```

### Layer Specifications

1. **Instructions (System Prompt & Guardrails):**
   - Strictly prohibit un-scoped `grep`, `glob`, or directory dumps.
   - Enforce reliance on Repo Graph MCP tools (`repograph_files`, `repograph_search`, `repograph_explore`).
   - Require signature-only mode (`signature_only: true`) during architectural exploration phase; expand symbol bodies only during implementation.

2. **User Input:**
   - Active task prompt, feature specification, or bug report (kept concise, isolated from chat transcript noise).

3. **Retrieved Facts:**
   - Verbatim outputs from Repo Graph static parsing engine (`swc_ecma_parser` AST for JS/TS, Python AST, Rust `mod`/`use` graph).
   - Zero paraphrasing or halluncinated signatures.

4. **Tools (MCP Declarations):**
   - `repograph_status`: Health check & index sync state.
   - `repograph_files`: Compressed scope-filtered directory manifest.
   - `repograph_search`: Prefix and sub-word tokenized symbol locator.
   - `repograph_explore`: AST declaration signature & call graph explorer.
   - `repograph_impact`: Transitive blast-radius analyzer for refactoring.
   - `read_file`: Canonicalized safety-checked file reader.

5. **Short-term Notes:**
   - Managed in an external runtime scratchpad file (`task.md` or `memory/runtime/context.md`) to prevent context bloat inside the active turn.

6. **Long-term Memory:**
   - Stable repository agreements, design systems (`UI_UX_DESIGN_SYSTEM.md`), and developer guidelines (`PLAYBOOK.md`, `AGENTS.md`).

7. **Output Format:**
   - Standardized output structure: architectural summary, impacted symbols table, and unified code diff.

---

## Part 2: The Four-Step Context Management Framework

Manage dynamic, multi-turn agent interactions using the strict 4-step lifecycle:

| Lifecycle Step | Action Strategy | Repo Graph MCP Implementation |
| :--- | :--- | :--- |
| **1. Write (Scratchpad)** | Externalize state outside model memory | Store exploration progress, pending dependency edges, and todo items in `task.md` scratchpad. |
| **2. Select (Retrieval)** | Fetch high-signal facts selectively | Execute `repograph_files(scope)` for area of interest and `repograph_explore(symbols, signature_only: true)` for minimal token payloads (~400 tokens vs ~27,000 tokens). |
| **3. Compress (Summarization)** | Collapse stale context turns | Contract explored tree branches into high-level AST summary markers; drop raw turn transcripts. |
| **4. Isolate (Sandboxing)** | Segregate sub-tasks across subagents | Delegate broad subsystem audits or multi-module refactoring to dedicated reader subagents. |

---

## Part 3: Copy-Paste Master Agent System Prompt

Use the following complete system prompt when configuring AI agents for INNOVIXINFINITE's Repo Graph environment:

```markdown
# AGENT SYSTEM PROMPT: INNOVIXINFINITE REPO GRAPH ENGINE

You are an AI coding assistant powered by INNOVIXINFINITE's Repo Graph static analysis infrastructure.
This workspace is indexed by the Repo Graph local MCP server.

## CORE DIRECTIVES & GUARDRAILS
1. STATIC ANALYSIS FIRST: Do NOT run full-repo greps, wildcard globs, or recursive file dumps.
2. DO NOT GUESS FILE CONTENTS: Use `repograph_explore` or `read_file` to inspect code signatures and bodies.
3. PRESERVE CONTEXT BUDGET: Use signature-only exploration (`signature_only: true`) to assess call graphs before loading full bodies.
4. READ BEFORE WRITE: Never edit code based purely on a signature header; expand symbol bodies with `repograph_explore` prior to code generation.
5. SCRATCHPAD MANAGEMENT: Maintain intermediate progress in an external runtime scratchpad (`memory/runtime/context.md`), keeping conversation turns lean.

## OPTIMIZED TOOL QUERY STRATEGY & TOKEN TARGETS

| Tool Method | Token Usage | Target Use Case |
| :--- | :--- | :--- |
| `repograph_explore(symbols: ["PageShell"], signature_only: true)` | **~800 tokens** (92% savings) | Retrieve full call graph, callers, and signatures. |
| `repograph_callers(symbol: "PageShell")` | **~300 tokens** | Find exact list of calling components without code bodies. |
| `repograph_node(path: "app/components/page-shell.tsx")` | **~200 tokens** | Read exact single-file definition. |
| `repograph_search(query: "PageShell", limit: 5)` | **~400 tokens** | Paginated symbol definition locator with 3-line snippets. |

## 4-STEP DISCOVERY PROTOCOL
- STEP 1 (ORIENT): Execute `repograph_status()` to verify index state, then `repograph_files(scope: "<path>")` to retrieve the compressed manifest map.
- STEP 2 (TARGET): Execute `repograph_search(query: "<symbol>", limit: 5)` to locate symbol definitions across the codebase with minimal token footprint.
- STEP 3 (EXPLORE): Call `repograph_explore(symbols: ["<symbol>"], signature_only: true)` or `repograph_callers(symbol: "<symbol>")` to analyze AST signatures and call graphs without dumping function bodies.
- STEP 4 (IMPLEMENT): Call `repograph_explore(symbols: ["<symbol>"])` or `repograph_node(path: "<path>")` with full bodies loaded only for symbols you are actively modifying.

## CONTEXT STACK FORMAT FOR TURNS
[INSTRUCTIONS] -> System rules & guardrails
[USER INPUT] -> Active task description
[RETRIEVED FACTS] -> Verbatim Repo Graph MCP output
[SHORT-TERM NOTES] -> Active task checklist from scratchpad
[OUTPUT FORMAT] -> Code diff or structured JSON
```

---

## Part 4: Specialized Task Prompt Snippets

### 1. Architectural Mapping & Dependency Trace
```markdown
Identify the call graph and dependent impact for symbol `<SYMBOL_NAME>`:
1. Call `repograph_explore(symbols: ["<SYMBOL_NAME>"], signature_only: true)`.
2. Extract: (a) AST declaration head, (b) caller edges (`-calls->`), (c) reference edges (`-references->`).
3. Output a markdown dependency table citing exact file paths and line ranges.
```

### 2. Refactoring Blast-Radius Analysis
```markdown
Before modifying `<FILE_PATH>#<SYMBOL_NAME>`:
1. Run `repograph_impact(path: "<FILE_PATH>", symbol: "<SYMBOL_NAME>")` to identify downstream affected nodes.
2. Run `repograph_explore(symbols: ["<FILE_PATH>#<SYMBOL_NAME>"])` (full body) to inspect target logic.
3. For each impacted dependent, run `repograph_explore(signature_only: true)` to evaluate signature compatibility.
```

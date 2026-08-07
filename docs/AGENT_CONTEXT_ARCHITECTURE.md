# Agent Context Engineering & MCP Architecture (`docs/AGENT_CONTEXT_ARCHITECTURE.md`)

This document outlines the **Context Engineering Architecture** for **Repo Graph**. It provides a 1-to-2 call exploration strategy designed to expose the codebase dependency graph to AI agents (**Claude Code, Cursor, Codex, OpenCode, Hermes, Gemini, Antigravity, and Kiro**) over Model Context Protocol (MCP).

---

## 1. The Seven-Piece Context Stack

```
┌────────────────────────────────────────────────────────────────────────────────────────┐
│ 1. INSTRUCTIONS     │ 1-2 Call Execution Budget centered around repograph_explore      │
├─────────────────────┼──────────────────────────────────────────────────────────────────┤
│ 2. USER INPUT       │ Task Intent Classification (Explore vs Search vs Impact)        │
├─────────────────────┼──────────────────────────────────────────────────────────────────┤
│ 3. RETRIEVED FACTS  │ Exact JSON payload (files, code_blocks, paths) from explore      │
├─────────────────────┼──────────────────────────────────────────────────────────────────┤
│ 4. TOOLS            │ Exact 8 `repograph_*` Tool Schemas & REPOGRAPH_MCP_TOOLS env     │
├─────────────────────┼──────────────────────────────────────────────────────────────────┤
│ 5. SHORT-TERM NOTES │ Context footprint ledger & token savings tracking                │
├─────────────────────┼──────────────────────────────────────────────────────────────────┤
│ 6. LONG-TERM MEMORY │ SWC AST parser (JS/TS) & Extractor string scanners (Others)     │
├─────────────────────┼──────────────────────────────────────────────────────────────────┤
│ 7. OUTPUT FORMAT    │ Deterministic 3-step response schema                             │
└────────────────────────────────────────────────────────────────────────────────────────┘
```

### Layer 1: Instructions (Core Agent Rules & Constraints)
- **Rule 1 (Explore First):** For any task involving functions, endpoints, or components, your **first and primary tool call** must be `repograph_explore(symbols: ["SymbolA", "path/file.ts#SymbolB"])`.
  - `repograph_explore` returns exact source code blocks AND bi-directional call-graph edges (`paths`) in **one single round-trip**.
  - You may supply bare symbol names (`"init_db"`) or fully qualified path-symbol pairs (`"src-tauri/src/db.rs#init_db"`) to disambiguate identical symbol names.
- **Rule 2 (1-to-2 Call Execution Budget):**
  - *Call 1 (Primary):* `repograph_explore(symbols: [...])` or `repograph_search(query: "...")` -> Obtains code slices + call graph in one turn.
  - *Call 2 (Optional):* `repograph_impact(path, symbol)` -> Run blast-radius analysis before high-risk refactoring.
- **Rule 3 (Zero Unknown Tool Errors):** Always use exact wire tool names prefixed with `repograph_` (`repograph_explore`, `repograph_node`, `repograph_files`, `repograph_search`, `repograph_impact`). Never call `get_manifest` or `read_file`.
- **Rule 4 (Environment Variable Alignment):** Ensure client configs specify `REPOGRAPH_MCP_TOOLS=explore,files,node,search,impact,callers,callees,status` so all 8 tools are exposed.

---

### Layer 2: User Input (Task Intent Classification Matrix)

| User Task Pattern | Preferred Tool Sequence | Target Call Count |
| :--- | :--- | :--- |
| **"Where is X defined / how does Y call Z?"** | `repograph_explore(symbols: ["X", "Y"])` | **1 Call** |
| **"Find symbols matching term..."** | `repograph_search(query: "term")` — returns full source per match; see caveats | **1 Call** |
| **"What will break if I refactor X?"** | `repograph_impact(symbol: "X")` | **1 Call** |
| **"Modify feature X"** | `repograph_explore` -> Apply Edit | **2 Calls** |

---

### Layer 3: Retrieved Facts (Actual `repograph_explore` JSON Payload Format)

When calling `repograph_explore(symbols: ["init_db"])`, the agent receives the exact JSON wire payload:

```json
{
  "files": [
    {
      "path": "src-tauri/src/db.rs",
      "code_blocks": [
        {
          "name": "init_db",
          "kind": "function",
          "code": "pub fn init_db(db_path: &Path) -> Result<Connection> {\n  // exact source slice (heuristic scan for Rust; AST-exact for JS/TS)\n}",
          "start_line": 6,
          "end_line": 116
        }
      ]
    }
  ],
  "paths": [
    {
      "from_symbol": "src-tauri/src/db.rs#reconcile_repo_startup",
      "to_symbol": "src-tauri/src/db.rs#init_db",
      "kind": "calls"
    }
  ]
}
```

> **Parsing Note:** Callers and callees are merged into the flat `paths` array. Upstream callers have `to_symbol == Target`, while downstream callees have `from_symbol == Target`.

#### Response Format Is Not Uniform Across Tools

Only two of the eight tools return JSON. The rest return plain text. Agents must not assume a JSON envelope.

| Tool | Response format | Verified shape |
| :--- | :--- | :--- |
| `repograph_explore` | **JSON object** | `{ files: [{ path, code_blocks: [{ name, kind, code, start_line, end_line }] }], paths: [{ from_symbol, to_symbol, kind }] }` |
| `repograph_search` | **JSON array** | `[{ name, file_path, content }]` — `content` is the **full symbol source**, not a snippet |
| `repograph_files` | Markdown text | `## Project Architecture Map` + one line per file + `## Warnings` |
| `repograph_status` | Markdown text | `## CodeGraph Status` + root, sync state, pending files |
| `repograph_node` | **Raw source text** | The bare file or symbol body — no wrapper, no metadata, no line numbers |
| `repograph_callers` | Plain text list | `Callers of symbol 'X':` then `- path#symbol (kind)` |
| `repograph_callees` | Plain text list | `Callees of symbol 'X':` then `- path#symbol (kind)` |
| `repograph_impact` | Plain text list | `Blast radius / Impact of changing symbol 'X':` then `- path#symbol` (no kind) |

**Verified examples** (`buildFlow` in `src/lib/layout.ts`):

```text
# repograph_callers(path: "src/lib/layout.ts", symbol: "buildFlow")
Callers of symbol 'buildFlow':
- src/components/GraphCanvas.tsx#src/components/GraphCanvas.tsx (file)
- src/components/GraphCanvas.tsx#Canvas (component)

# repograph_impact(path: "src/lib/layout.ts", symbol: "buildFlow")
Blast radius / Impact of changing symbol 'buildFlow':
- src/App.tsx#App
- src/components/GraphCanvas.tsx#Canvas
- src/components/GraphCanvas.tsx#GraphCanvas
```

#### Known Retrieval Caveats

- **`repograph_search` is expensive, not cheap.** Each result carries the entire source of the matched symbol in `content`. A one-word query can return thousands of tokens. Prefer `repograph_explore` when the symbol name is already known; reserve `search` for genuine name discovery, and expect full bodies back.
- **`repograph_callees` includes local variable declarations, not just invocations.** For `buildFlow` it returned 28 entries, of which 24 were in-function locals (`visible`, `dir`, `y`, `seen`, …) alongside the 2 real function calls (`normalizeLanguage`, `dirOf`). Treat `(function)` / `(component)` kinds as the actionable subset and filter `(variable)`.
- **File nodes appear as self-referential pseudo-symbols.** Entries like `src/App.tsx#src/App.tsx` (kind `file`) represent the file itself, not a symbol named after the path.
- **Misses fail silently — there is no "not found" error.** `repograph_explore(["nonexistent_xyz"])` returns `{"files":[],"paths":[]}` and `repograph_search` returns `[]`. A typo is indistinguishable from a symbol that genuinely has no matches, so an empty result must never be read as "this symbol does not exist." Re-query with `repograph_search` before concluding anything.
- **Bare symbol names are unreliable on the graph tools.** `repograph_callers(symbol: "main")` returned `No callers found for symbol 'main'`, while `repograph_callers(path: "src-tauri/src/main.rs", symbol: "main")` correctly returned its caller. Always pass `path` alongside `symbol` for `callers` / `callees` / `impact`. (`repograph_explore` does resolve bare names — `["buildFlow"]` worked — but path-qualified `'path#symbol'` remains safer for names that repeat across files.)

---

### Layer 4: Tools Specification (Exact 8 Wire Tools)

```json
{
  "tools": [
    {
      "name": "repograph_explore",
      "description": "PRIMARY COMPOUND TOOL. Accepts an array of symbol names or 'path#name' pairs. Returns code blocks and bi-directional call-graph paths in a single call.",
      "inputSchema": {
        "type": "object",
        "properties": {
          "symbols": {
            "type": "array",
            "items": { "type": "string" },
            "description": "List of symbol names or 'path#symbol' references (e.g. ['buildFlow', 'src-tauri/src/db.rs#init_db'])"
          }
        },
        "required": ["symbols"]
      }
    },
    {
      "name": "repograph_files",
      "description": "Fetch project file tree topology & manifest map.",
      "inputSchema": {
        "type": "object",
        "properties": {
          "scope": { "type": "string", "description": "Optional subdirectory prefix (e.g. 'src/components')" }
        }
      }
    },
    {
      "name": "repograph_node",
      "description": "Fetch source code of a file or a specific symbol within a file.",
      "inputSchema": {
        "type": "object",
        "properties": {
          "path": { "type": "string", "description": "Relative path to file in project" },
          "symbol": { "type": "string", "description": "Optional symbol name to slice" }
        },
        "required": ["path"]
      }
    },
    {
      "name": "repograph_search",
      "description": "Full-text symbol search across the repository, backed by the SQLite FTS5 `symbols_fts` index (prefix/token matching, not edit-distance fuzzy).",
      "inputSchema": {
        "type": "object",
        "properties": {
          "query": { "type": "string", "description": "Symbol name search query" }
        },
        "required": ["query"]
      }
    },
    {
      "name": "repograph_impact",
      "description": "Perform blast radius analysis to find all files/symbols impacted by changes to a file or symbol.",
      "inputSchema": {
        "type": "object",
        "properties": {
          "path": { "type": "string", "description": "File path" },
          "symbol": { "type": "string", "description": "Symbol name" }
        }
      }
    },
    {
      "name": "repograph_callers",
      "description": "Get upstream callers of a target file or symbol.",
      "inputSchema": {
        "type": "object",
        "properties": {
          "path": { "type": "string" },
          "symbol": { "type": "string" }
        }
      }
    },
    {
      "name": "repograph_callees",
      "description": "Get downstream callees invoked by a target file or symbol.",
      "inputSchema": {
        "type": "object",
        "properties": {
          "path": { "type": "string" },
          "symbol": { "type": "string" }
        }
      }
    },
    {
      "name": "repograph_status",
      "description": "Returns the active project root, connection sync state, and pending file details.",
      "inputSchema": { "type": "object", "properties": {} }
    }
  ]
}
```

---

### Layer 5: Short-Term Notes (Session Scratchpad)
- Track active symbol targets and turn metrics in `task.md`:
  ```markdown
  - Target Task: Refactor `buildFlow` column gaps
  - Action 1: `repograph_explore(symbols: ["buildFlow"])`
  - Result: implementation + call graph in 1 turn — 7,834 chars (~2,117 tok)
  ```

#### Measured Cost: `explore` Is Not Always Cheaper Than Reading the File

Savings depend entirely on how much of a file the target symbol occupies. Measured on this repo:

| Approach | Chars | Est. tokens |
| :--- | ---: | ---: |
| `repograph_explore(["buildFlow"])` — 4,614 code + 3,100 `paths` | 7,834 | ~2,117 |
| Plain full read of `src/lib/layout.ts` | 5,959 | ~1,611 |

`buildFlow` spans lines 33–169 of a 195-line file — **75% of the file** — so slicing recovers almost nothing, while the 30-entry `paths` array adds 3,100 chars (of which ~24 entries are the local-variable noise described above). For this symbol, `explore` costs **1.31× more** than simply reading the file.

**Rule of thumb:** `explore` wins decisively when the target symbol is a small fraction of a large file, or when you need the call graph anyway. It loses on symbol-dominant files. Do not assume a fixed savings percentage — the 90%+ figure applies to whole-repo manifest substitution (`repograph_files` instead of ingesting every file), not to individual `explore` calls.

---

### Layer 6: Long-Term Memory & Extraction Architecture

- **JS/TS Parsing:** AST-exact parsing powered by `swc_ecma_parser` (`swc_common`, `swc_ecma_ast`, `swc_ecma_visit`).
- **Rust, Python, & Other Languages:** Lightweight, hand-rolled string-scanning extractors implementing the common `Extractor` trait.
  - *Engineering Note:* JS/TS symbol boundaries are AST-exact. Non-JS/TS language symbols are extracted via heuristic regex/string scanning.
- **In-Memory Graph Cache:** Incremental re-indexing triggered dynamically via `notify` file-watcher events.

---

### Layer 7: Output Format (Standardized Response Schema)

```markdown
### 1. Codebase Finding
Brief explanation of where candidate symbols live.

### 2. Exploration Analysis (via `repograph_explore`)
Summary of implementation, callers, and callees.

### 3. Execution / Edits
Targeted patch or answer.
```

---

## 2. The 4-Step Context Management Framework

```
  ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
  │  1. WRITE       │    │  2. SELECT      │    │  3. COMPRESS    │    │  4. ISOLATE     │
  │ (Scratchpad)    │ ─> │ (repograph_     │ ─> │ (JSON Compound  │ ─> │ (Explorer vs    │
  │                 │    │  explore)       │    │  Payload)       │    │  Editor Roles)  │
  └─────────────────┘    └─────────────────┘    └─────────────────┘    └─────────────────┘
```

1. **Write (Scratchpad Isolation):** Store symbol targets in session notes (`task.md`) rather than cluttering conversation state.
2. **Select (Precision Retrieval):** Request only specific symbol slices via `repograph_explore` rather than ingesting full raw files.
3. **Compress (Compound Payload):** `repograph_explore` delivers files, code blocks, and graph edges (`paths`) in a single JSON structure. Note that the graph/list tools return plain text rather than JSON — see "Response Format Is Not Uniform Across Tools".
4. **Isolate (Multi-Agent Subsystems):** Isolate graph exploration from file edits using specialized subagents.

---

## 3. Multi-Agent Configuration Matrix

Seven config formats cover the eight supported agents — Gemini and Antigravity share the same file. To unlock all 8 tools across client environments, include `REPOGRAPH_MCP_TOOLS=explore,files,node,search,impact,callers,callees,status`:

### 1. Claude Code (CLI)
`~/.mcp.json` or `.mcp.json`:
```json
{
  "mcpServers": {
    "repo-graph": {
      "command": "C:/My-pro/project-map/src-tauri/target/debug/mcp_server.exe",
      "args": ["C:/My-pro/project-map"],
      "env": {
        "REPOGRAPH_MCP_TOOLS": "explore,files,node,search,impact,callers,callees,status"
      }
    }
  }
}
```

### 2. Cursor (IDE)
`.cursor/mcp.json`:
```json
{
  "mcpServers": {
    "repo-graph": {
      "command": "C:/My-pro/project-map/src-tauri/target/debug/mcp_server.exe",
      "args": ["C:/My-pro/project-map"],
      "env": {
        "REPOGRAPH_MCP_TOOLS": "explore,files,node,search,impact,callers,callees,status"
      }
    }
  }
}
```

### 3. Codex (Desktop / CLI)
`C:\Users\<username>\.codex\config.toml`:
```toml
[mcp_servers.repo_graph]
command = 'C:\My-pro\project-map\src-tauri\target\debug\mcp_server.exe'
args = ['C:\My-pro\project-map']
startup_timeout_sec = 120

[mcp_servers.repo_graph.env]
REPOGRAPH_MCP_TOOLS = "explore,files,node,search,impact,callers,callees,status"
```

### 4. OpenCode
`opencode.json`:
```json
{
  "mcp": {
    "servers": {
      "repo-graph": {
        "command": "C:/My-pro/project-map/src-tauri/target/debug/mcp_server.exe",
        "args": ["C:/My-pro/project-map"],
        "env": {
          "REPOGRAPH_MCP_TOOLS": "explore,files,node,search,impact,callers,callees,status"
        }
      }
    }
  }
}
```

### 5. Hermes
`hermes.config.json`:
```json
{
  "mcp_providers": [
    {
      "name": "repo-graph",
      "executable": "C:/My-pro/project-map/src-tauri/target/debug/mcp_server.exe",
      "arguments": ["C:/My-pro/project-map"],
      "environment": {
        "REPOGRAPH_MCP_TOOLS": "explore,files,node,search,impact,callers,callees,status"
      }
    }
  ]
}
```

### 6. Gemini / Antigravity
`C:\Users\<username>\.gemini\antigravity\mcp_config.json`:
```json
{
  "mcpServers": {
    "repo-graph": {
      "command": "C:/My-pro/project-map/src-tauri/target/debug/mcp_server.exe",
      "args": ["C:/My-pro/project-map"],
      "env": {
        "REPOGRAPH_MCP_TOOLS": "explore,files,node,search,impact,callers,callees,status"
      }
    }
  }
}
```

### 7. Kiro
`.kiro/mcp_config.json`:
```json
{
  "mcpServers": {
    "repo-graph": {
      "command": "C:/My-pro/project-map/src-tauri/target/debug/mcp_server.exe",
      "args": ["C:/My-pro/project-map"],
      "env": {
        "REPOGRAPH_MCP_TOOLS": "explore,files,node,search,impact,callers,callees,status"
      }
    }
  }
}
```

---

## 4. Production Master System Prompt (Verified & Tested)

Paste this instruction block into system prompt or agent rules files (`AGENTS.md`, `.cursorrules`, `CLAUDE.md`, etc.):

```markdown
# AGENT INSTRUCTION: REPO-GRAPH HIGH-EFFICIENCY CODEBASE EXPLORATION

You are equipped with the Repo Graph MCP Server (`repo-graph`). Your objective is to perform codebase navigation and refactoring in 1 TO 2 TOOL CALLS MAX using the compound tool `repograph_explore`.

## CORE WIRE TOOL CONTRACT (8 Tools Available)

- `repograph_explore(symbols: string[])` -> ⚡ PRIMARY COMPOUND TOOL. Call this first! Accepts bare names ('buildFlow') or 'path#symbol' references. Returns JSON containing `files` (code blocks) and `paths` (call graph edges).
- `repograph_search(query: string)` -> Full-text (FTS5) symbol name search. WARNING: returns the FULL SOURCE of every match. Use only when the symbol name is unknown; prefer `repograph_explore` otherwise.
- `repograph_files(scope?: string)` -> View compressed directory topology map (markdown text).
- `repograph_node(path: string, symbol?: string)` -> Fetch specific file or symbol content. Returns RAW SOURCE TEXT with no wrapper or line numbers.
- `repograph_impact(path?: string, symbol?: string)` -> Run blast-radius analysis before refactoring.
- `repograph_callers(path?: string, symbol?: string)` -> Fetch upstream caller graph.
- `repograph_callees(path?: string, symbol?: string)` -> Fetch downstream callee graph.
- `repograph_status()` -> Returns active project root, sync state, and pending (unindexed) file details.

## 1-TO-2 CALL EXECUTION STRATEGY

1. TURN 1 (PRIMARY): Call `repograph_explore(symbols: ["TargetSymbol"])` or `repograph_search(query: "target")`.
   - Do NOT call `get_manifest` or `read_file` (these are internal Rust names, not wire tool names).
   - `repograph_explore` returns code blocks and bi-directional call-graph edges (`paths`) in a single JSON payload.
2. TURN 2 (EDIT/RESPONSE): Apply targeted code modifications or present your answer to the user.
3. FOR HIGH-RISK INTERFACE CHANGES: Call `repograph_impact(symbol: "TargetSymbol")` before editing to inspect downstream impact.

## RESPONSE PARSING RULES

- Only `repograph_explore` (JSON object) and `repograph_search` (JSON array) return JSON. All other tools return plain text or markdown — do not attempt to JSON-parse them.
- In `repograph_explore`, callers and callees are merged in the flat `paths` array: a caller has `to_symbol == target`, a callee has `from_symbol == target`.
- `repograph_callees` lists in-function local variables alongside real calls. Filter for `(function)` / `(component)` kinds; ignore `(variable)`.
- Entries shaped `path#path` (kind `file`) are the file node itself, not a symbol.
- An empty result (`{"files":[],"paths":[]}` or `[]`) is NOT proof the symbol is absent — misses fail silently. Re-query with `repograph_search` before concluding a symbol does not exist.
- For `repograph_callers` / `callees` / `impact`, always pass `path` together with `symbol`. Bare `symbol` alone can return "No callers found" for symbols that do have callers.

Never browse raw source files speculatively. Prefer `repograph_explore` when the target symbol is a small part of a large file; for symbol-dominant files a plain read can be cheaper.
```

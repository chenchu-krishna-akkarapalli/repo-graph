# Logic: Agent API (MCP Server)

Owns: exposing the graph + manifest + file access as MCP tools that Claude
Desktop, Claude Code, or any other MCP-compatible agent can call.

## Tools exposed

### `get_manifest(scope?: string)`
- No args → full-repo manifest (see `token-optimization-logic.md` for
  format).
- `scope` (glob-like path prefix) → subgraph manifest for just that area.
- Triggers a re-index if the internal graph is stale (see caching below);
  otherwise serves from cache.

### `repograph_node(path: string, symbol?: string, start_line?: number, end_line?: number, with_line_numbers?: boolean)`
- Reads either the raw file content or symbol block.
- `start_line` / `end_line`: 1-indexed line numbers for targeted slicing.
- `with_line_numbers`: format output with line numbers (e.g. `42: export function...`). Defaults to true when sliced.

### `repograph_edit(path: string, target_content: string, replacement_content: string)`
- Closed-loop atomic string/symbol replacer with CRLF/LF normalization.
- Pre-write AST syntax validation rejects malformed edits without touching disk.
- Synchronously updates the internal AST graph (0ms watcher lag) and returns real-time edge diffs and embedded in-band syntax `diagnostics`.

### `repograph_batch_edit(patches: Array<{path: string, target_content: string, replacement_content: string}>)`
- Atomic multi-file refactoring across multiple files with pre-validation and rollback guarantee.
- Validates all file targets and AST syntax in memory before applying any disk writes.

### `repograph_edit_symbol(path: string, symbol: string, new_code: string)`
- Replaces the exact source range of an AST symbol in a file without requiring agents to send large string blocks or manual line offsets.

### `repograph_write(path: string, content: string)`
- File creation/overwrite tool with recursive parent directory creation.
- Pre-write AST validation and synchronous AST parsing and edge indexing.

### `repograph_delete(path: string)`
- Pruning tool for files or directories with automatic dependency graph unmounting.

### `repograph_search(query: string, limit?: number, signature_only?: boolean, exact_symbol_only?: boolean, force_full?: boolean)`
- Searches for symbols in the SQLite FTS5 index.
- `limit` (default: 10) prevents context flooding on common component matches.
- `signature_only` (default: true) returns concise declaration signatures instead of dumping full 250+ line function/component bodies.
- Body matches return a targeted 3-line grep context snippet rather than full file ASTs.
- `exact_symbol_only` (default: false) matches exact symbol identifier names.
- **Intelligent Context Throttling**: If calculated payload exceeds 2,500 tokens and `force_full: false`, automatically compresses output to signature-only mode with an auto-budget notice.

### `repograph_explore(symbols: string[], signature_only?: boolean, compact_edges?: boolean, force_full?: boolean)`
- Multi-symbol call-graph traversal and AST slice reader.
- `signature_only: true` returns compact declaration heads + call graphs (~800 tokens for entire components vs ~10,000 tokens uncompressed).
- **Intelligent Context Throttling**: If calculated payload exceeds 2,500 tokens and `force_full: false`, automatically compresses output to signature-only mode with an auto-budget notice.

### `repograph_impact(path?: string, symbol?: string)`
- Computes transitive blast radius (dependents chain) for a modified symbol or file.

### `repograph_callers(path?: string, symbol?: string)`
- Returns incoming callers/dependents for a symbol or file.

### `repograph_callees(path?: string, symbol?: string)`
- Returns outgoing dependencies/callees for a symbol or file.

## Persistent Session & Connection Management

- **Resident Session**: The MCP server maintains active session state across all multi-turn agent conversations without dropping connection context.
- **Keepalive / Heartbeat**: The JSON-RPC `ping` method records heartbeats and returns health metrics (`connected: true`, `session_active: true`, `session_id`, `uptime_ms`).
- **Connection Health Tool (`repograph_status`)**: Exposes connection state (`connected: true`, `session_active: true`), session uptime, heartbeats, tool queries, and pending sync files.

## Database & In-Memory Concurrency (v3.0)

- **SQLite WAL Mode & Busy Timeout**: Connection initialization sets `PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 5000; PRAGMA synchronous = NORMAL;`, preventing `SQLITE_BUSY` crashes during concurrent agent queries.
- **0ms Watcher Lag (Synchronous In-Memory AST Mounting)**: Mutations synchronously update SQLite and reload memory graph before the MCP response is returned, clearing pending flags immediately.

## Security / safety constraints

- **Closed-Loop Mutator Safety.** All modifications are confined strictly within the repository root. AST validation runs prior to writing files to disk to prevent corrupted states.
- **No code execution during parsing**, ever — static analysis only, even for frameworks that would technically let you introspect routes by importing and running the app.
- **Repo root confinement** — every path argument is canonicalized and checked against the repo root before any file read or write.

## Example agent interaction (end to end closed-loop v3.0)

1. Agent receives task: "Refactor theme hook across multiple components."
2. Agent calls `repograph_explore(symbols: ["useTheme"])`.
3. Agent checks blast radius with `repograph_impact(symbol: "useTheme")`.
4. Agent performs atomic multi-file refactoring using `repograph_batch_edit(patches: [...])`.
5. Server pre-validates AST syntax for all targets, writes atomically, re-indexes graph synchronously in memory, and returns edge diff and in-band diagnostics.
6. Done — 100% closed-loop workflow completed via MCP tools with 0 token waste and zero watcher lag.

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

### `read_file(path: string)`
- Returns raw file contents.
- **Must validate** `path` resolves inside the repo root — reject path
  traversal (`../../etc/passwd`-style) outside the indexed root.
- Should be a thin, fast passthrough — no transformation of the file
  content.

### `find_dependents(path: string)`
- Returns all files with an edge into `path`. Used before risky edits
  ("what breaks if I change this").

### `find_route(pattern: string)`
- Resolves a URL/route string to the file(s) implementing it (web
  frameworks only — Next.js App Router, FastAPI, Flask).

## Caching / staleness

- Graph is built once on first `get_manifest()` call per session and cached
  in memory.
- A file-watcher (backend) invalidates just the changed file's node/edges
  rather than the whole graph on every keystroke — full rebuild only on
  structural changes (files added/removed) beyond a debounce threshold.
- `get_manifest()` includes a `generated_at` timestamp so the calling agent
  can reason about freshness if needed.

## Security / safety constraints

- **Read-only.** This MCP server never writes, executes, or modifies
  project files — it's a query layer, not an editing layer. Actual edits
  are made by the calling agent through its own file-write tools, not
  through this server.
- **No code execution during parsing**, ever — static analysis only, even
  for frameworks that would technically let you introspect routes by
  importing and running the app. This is a hard line, not a
  performance trade-off.
- **Repo root confinement** — every path argument is canonicalized and
  checked against the repo root before any file read.

## Error handling

- Malformed `path` → structured error, not a silent empty result, so the
  calling agent can retry or ask the user rather than assuming a file is
  empty/missing.
- Parse failures on individual files (syntax errors, unsupported syntax)
  should not fail the whole `get_manifest()` call — that file is included
  with an empty extraction result and a `warnings` entry, and everything
  else in the repo still gets mapped.

## Example agent interaction (end to end)

1. Agent receives task: "Fix the login button style."
2. Agent calls `get_manifest()`.
3. Agent matches "login button" against manifest entries — finds
   `Button.tsx` (component) and possibly `login.ts` (route) as candidates.
4. Agent calls `read_file("src/components/Button.tsx")`.
5. Agent makes the edit using its own file-write tool (outside this MCP
   server's scope).
6. Done — no other files in the repo were ever read.

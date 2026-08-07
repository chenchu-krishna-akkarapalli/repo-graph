# Playbook

Operating guide for building and evolving Repo Graph. This is the "how we
work" doc — pair it with `CLAUDE.md` (context) and `logic/` (subsystem specs).

## 1. Proof-of-Concept Roadmap

Build in this order — each step is independently testable.

### Step 1 — File Tree Walker (Rust backend)
- Multi-threaded directory traversal (`ignore` or `walkdir` crate)
- Skip: `node_modules/`, `.git/`, `dist/`, `build/`, `target/`, lockfiles,
  binary/media assets
- Output: flat list of `{path, size, extension}`
- **Done when:** can index a 10k-file repo in well under a second.

### Step 2 — Import/Export Extraction
- Start with regex/line-scanning for a quick PoC, then upgrade to AST
  (`swc_ecma_parser` for JS/TS) once the pipeline is proven
- Resolve relative imports (`./component`) to absolute paths in-repo
- Output: edge list `{from, to, kind}` (kind = import/require/mod/use)
- **Done when:** edges correctly resolve for a real multi-folder JS project.

### Step 3 — Graph Construction + Tauri Bridge
- Combine walker output + edges into a directed graph (JSON-serializable)
- Pass across the Tauri IPC bridge to the frontend
- **Done when:** frontend receives a valid graph object on app load.

### Step 4 — Visualizer (React Flow)
- Render nodes (files) and edges (dependencies)
- Click a node → show file size, exports, dependents
- **Done when:** you can visually trace a dependency chain in the UI.

### Step 5 — Agent-Facing API (MCP Server)
- Wrap the graph + a `read_file(path)` tool behind an MCP server
- Serve the compressed manifest as the primary payload; full file contents
  only on explicit request
- **Done when:** Claude Desktop/Code can query the graph and fetch a single
  file without being handed the whole repo.

## 2. Language Support Priority

1. JavaScript/TypeScript (highest agent demand)
2. Python (FastAPI/Flask route markers as entry points)
3. Rust (Cargo.toml + mod/use)
4. Others as needed — always via the common `Extractor` trait, never bespoke
   one-offs bolted onto the core walker.

## 3. Conventions

- **No regex where an AST parser is feasible and fast enough** — regex is a
  PoC shortcut, not a long-term strategy (multi-line imports break it).
- **Every parser module returns the same shape**: `{exports: [], imports: [],
  entry_points: []}`. This keeps the graph builder language-agnostic.
- **Manifest output is markdown-first, JSON-backed.** The JSON is the source
  of truth; markdown is a rendering of it for agent system prompts (see
  `logic/token-optimization-logic.md` for the exact format).
- **Version the manifest schema.** Any field addition/removal to the JSON
  graph bumps `schema_version`.

## 4. Testing Strategy

- Unit tests per parser module against fixture repos (small, hand-crafted
  multi-file projects with known expected edges)
- Integration test: run the full pipeline against a real open-source repo,
  assert no crashes and a non-trivial edge count
- Regression test: snapshot the manifest output for a fixture repo; flag
  diffs on schema or extraction-logic changes

## 5. Definition of Done (for PoC)

- [ ] Walker indexes a real repo, correctly excluding noise directories
- [ ] JS/TS imports resolved to absolute in-repo paths
- [ ] Graph JSON renders in the React Flow UI
- [ ] Agent can query the graph via MCP and fetch a single file's contents
- [ ] Manual test: ask an agent "fix the login button style" against a demo
      repo and confirm it reads only the 1–2 relevant files, not the whole
      project

## 6. Open Questions to Resolve Early

- How to handle monorepos with multiple package roots?
- How to keep the graph fresh on file changes — full re-index, or
  incremental updates via file watcher?
- Should the manifest include function-level granularity, or stay
  file-level for v1? (Recommendation: file-level for v1, function-level as a
  stretch goal once the core loop is proven.)

## 7. Multi-Project & File Explorer Integration

To transition the app from single-repo to a general desktop workspace:

### 7.1 Backend API Expansion (Rust)
- **`open_project_dialog` Command:** Calls Tauri's native folder dialog. Returns the canonical path of the selected folder.
- **`index_and_load_graph` Command:** Combines the indexer and graph loader. Walks, parses, and constructs the graph for the new workspace, updates `.repograph/graph.json`, and returns the graph data.
- **`read_directory_tree` Command:** Walks the selected folder, filtering ignored folders, and returns a JSON-serializable node tree representing folders and files for left-sidebar explorer rendering.

### 7.2 Frontend Integration (React)
- **Left Sidebar Component:** Displays the workspace root name and a tree component representing folders and files. Clicking files selects the corresponding React Flow node, and double-clicking focuses/centers the node.
- **Global Project State:** A unified Zustand slice (`projectSlice`) managing:
  - `activeProjectRoot: string | null`
  - `fileTreeNodes: FileTreeNode[]`
  - `isIndexing: boolean`
- **Tauri dialog package:** Bind button interactions to `@tauri-apps/api/dialog` or invoke backend folder picking commands.

## 8. Multi-Language Parsers, Token Meter, and Editor Path Bug Fix

### 8.1 Token Savings & MCP Connection UI
- **Connection Helper Tab:** Add an "Integrate Agent" button in the Left Sidebar to open a modal showing:
  - Copyable configuration block for `claude_desktop_config.json`.
  - Copyable command: `claude mcp add repo-graph -- <binary_path> <active_project_root>`.
- **Token Burner Meter:** Show a real-time comparison bar:
  - *Full Ingest (Baseline):* Sum of `FileNode.size_bytes / 3.7` (estimated tokens for entire repo).
  - *Active Ingest:* `Manifest Size + sum(ReadFiles.size_bytes / 3.7)`.
  - *Savings %:* \(\text{Savings} \% = \left(1 - \frac{\text{Active Ingest}}{\text{Full Ingest}}\right) \times 100\). Rendered as an interactive progress bar showing token conservation.

### 8.2 Expanded Language Support
Modify `walker.rs` to allow indexing and implement light regex/line-scanning extractors in `parsers/` for:
- **Go (`.go`):** Scan `import` blocks and `package` declarations.
- **Java / Kotlin (`.java`, `.kt`):** Scan `import` and `package` lines.
- **C/C++ (`.c`, `.cpp`, `.h`, `.hpp`):** Scan `#include` paths (relative `""` vs system `<>`).
- **Swift (`.swift`):** Scan `import` lines.
- **HTML / Angular (`.html`):** Scan scripts and template links.
- **SQL / Prisma (`.sql`, `.prisma`):** Scan schema relations (`REFERENCES` / `relation`).
- **Dockerfile (`Dockerfile`):** Scan base images `FROM` and local copies `COPY`.
- **Markdown (`.md`):** Scan relative links `[text](./file.md)` as graph edges.

### 8.3 VS Code "File Does Not Exist" Fix
- **The Issue:** Opening a file launches `code <path>`. Using relative paths fails if VS Code's active window directory differs from the project root.
- **The Fix:** Ensure the `open_in_editor` Tauri command canonicalizes the path by joining the relative file path to the absolute `activeProjectRoot` before spawning the `code` shell process. Windows backslashes must be correctly escaped.

## 9. Visual Context Workspace & Scoped Prompt Exporter

To further optimize agent token costs, the application implements a visual curation environment allowing users to compile and export extremely compact, task-specific prompt contexts.

### 9.1 Global State (Zustand & store.ts)
- **`contextFiles` State:** A `Set<string>` containing paths of files manually selected for the active coding task.
- **Actions:**
  - `addFileToContext(path)`
  - `removeFileFromContext(path)`
  - `clearContextWorkspace()`
- **Dynamic Telemetry:** Calculate the aggregate token weight of the selected workspace files plus their localized dependency subgraph manifest.

### 9.2 Frontend Workspace UI (React)
- **Left Sidebar Addition:** Under the File Explorer, render a collapsible **"Context Workspace"** panel.
- **Features:**
  - Shows list of added files with individual token/character weights.
  - Displays a **"Total Context Size"** progress bar showing token weight relative to the target models (e.g. 8K / 32K context slots).
  - Hover states on file rows in the Explorer show a "+" button to add them directly to the workspace.
  - Double-clicking nodes or selecting them on the React Flow canvas allows clicking a toolbar button to "Add Selected to Context".
  - **"Copy Context Prompt" Button:** Copies a structured Markdown block directly to the clipboard.

### 9.3 Scoped Prompt Exporter Layout
The copied prompt follows the **Seven-Piece Context Stack** guidelines to keep token weight minimal:
1. **Instructions:** Directs the agent to focus strictly on the provided files.
2. **Subgraph Manifest:** Includes a localized manifest containing *only* the selected files, their direct imports, and their dependents (omitting the rest of the project structure).
3. **Selected Code:** Safely appends the actual source code contents of the selected files (read via Tauri `read_file` IPC or local state cache) in fenced code blocks.
4. **Active Task:** A placeholder header directing the user to write their task descriptions at the bottom.

## 10. Code Layout Exporter

To help agents quickly grasp the file organization structure on boot, the app supports exporting a formatted ASCII file tree.

### 10.1 Export Icon in Left Sidebar Header
- An export icon button (using `lucide-react`'s `Share2` or `FileSpreadsheet` or a folder icon, let's use `FolderTree` or `ClipboardCopy` with a tooltip *"Copy Code Layout"*) is positioned in the Left Sidebar header next to the workspace root title.
- Clicking the button dynamically constructs the ASCII representation of the active file tree and copies it to the clipboard.

### 10.2 ASCII Tree Generation Specification
- The generator reads the active project's `fileTree` data.
- It recursively formats nodes using standard ASCII tree characters (`├──`, `└──`, `│   `).
- Directories must end with a trailing slash (e.g. `src/`).
- The output must be formatted as:
  ```markdown
  ## Code Layout
  ```
  [project_folder_name]/
  ├── file_a.ts
  └── src/
      ├── main.tsx
      └── App.tsx
  ```
  ```
- Tooltip/Icon transition: The button changes state/color temporarily (for 1 second) to display a checkmark or copy confirmation to satisfy the **Feedback Principle**.

## 11. Multi-Project Synchronization & Overlapping Bug Prevention

When a developer operates across multiple repositories, a synchronization mismatch (the "overlapping bug") can occur if the visualizer GUI is focused on one repository while the agent's background MCP server remains locked to another.

To resolve this, the system implements a two-layered synchronization bridge:

### 11.1 Shared Active Project Registry
- **File Registry:** A shared JSON file located at a canonical global user path, specifically:
  `C:\Users\nmahe\.gemini\antigravity\active_project.json` (based on the user config App Data Directory).
- **Format:**
  ```json
  {
    "active_project_root": "C:/My-pro/project-map",
    "last_synced_at": "2026-07-19T03:17:05Z"
  }
  ```
- **Tauri GUI Writer:** Whenever the Tauri desktop application opens, indexes, or switches to a project folder (via `openProject` or on startup `load`), it writes the selected absolute canonical directory path to `active_project.json`.
- **MCP Server Reader:** Whenever `mcp_server.exe` starts or receives a tool call, if it is invoked in "dynamic sync mode" (e.g., when the path argument is set to `"auto"` or when no argument is specified), it reads `active_project.json` to dynamically bind its queries (`get_manifest`, `read_file`) to the project currently active in the visualizer GUI.

### 11.2 Agent Manifest Verification Constraint
- **Root Header in Manifest:** The `get_manifest` tool includes the active repository root path in its output headers:
  ```markdown
  ## Project Architecture Map (Active Root: C:/My-pro/project-map)
  ```
- **RULES.md Boundary Assertion:** Add a strict validation constraint to [RULES.md](file:///c:/My-pro/project-map/my-agent/RULES.md):
  - **Rule:** *"You must check that the 'Active Root' in the manifest header matches your current terminal directory. If they do not match, warning the user and refusing to make edits is mandatory."*

## 12. Advanced Symbol-Level Extraction, Resolution & Live Sync

To further reduce token costs and achieve deeper context engineering precision, the application is upgraded from file-level granularity to a **symbol-level database and resolver** with a live file watcher.

```
files ──► Symbol Extraction (swc / tree-sitter) ──► sqlite DB (.repograph/graph.db)
                  │
                  ▼
          Symbol Resolution (calls ──► definitions, React/JSX component bridges)
                  │
                  ▼
          Symbol-Level Queries & Context Curation (read_symbol, call graph maps)
```

### 12.1 Symbol-Level Extraction & Storage (SQLite)
- **Database Backend:** Migrate storage from a flat JSON cache to a local SQLite database (`.repograph/graph.db`) using Node's built-in `node:sqlite` in WAL mode (frontend) and `rusqlite` (Rust backend).
- **Sub-Nodes (Symbols):** Extractors must parse and log individual symbols inside files:
  - `NodeKind = File | Function | Class | Method | Struct | Interface | Type`
  - Record the symbol's name, line range (`start_line`, `end_line`), and parent file.
- **Reference Edges:** Scan for symbol references and function invocations (e.g. `foo()` or `<Button />`), creating an edge `{ from_symbol, to_symbol, kind: Call | Reference }`.

### 12.2 Resolution & Bridges
- **Imports Symbol Binder:** Resolve imports to specific symbol declarations (e.g. `import { execute }` binds the local reference directly to the declared function in the target module).
- **React/JSX Component Bridge:** Map JSX component elements (e.g., `<Header />` or `<DetailSidebar />`) to their respective file and default/named function exports.

### 12.3 Incremental Auto-Sync (Live File Watcher)
- **Watcher Engine:** In the Rust backend, run a background thread using the `notify` crate to watch the active project directory for file change events.
- **Debounced Incremental Parser:**
  - Debounce events by `500 ms`.
  - Filter changes to supported source files.
  - Re-parse **only** the modified files using the specific language extractor, update their symbol rows/edges in the SQLite database, and notify the frontend visualizer via Tauri event emission (`tauri::Event`).

### 12.4 Symbol-Level Curation & targeted Read Tool
- **Visual Call Graph Inspector:** In the Detail Sidebar, show a "Call Graph" tab rendering a tree of callers and callees for the selected symbol, allowing developers to visually audit execution flows.

## 13. Symbol-Level Visualizer, Call Graphs & Event Sync

This integration connects the frontend visualizer to the backend symbol database and watcher events, completing the visual curation pipeline.

### 13.1 Symbol-Level Sub-Flows (React Flow Canvas)
- **Nested Nodes:** File nodes can be visually expanded to render their internal symbols (functions, classes) as nested child nodes inside the file's bounding box.
- **Call Edges:** Render fine-grained edges linking function call nodes directly to their declaration targets across files, color-coding them distinct from file-level edges.
- **Zustand Selectors:** Keep symbol layouts cached, updating sub-nodes dynamically only when a file node is expanded.

### 13.2 Visual Call Graph Tab & Symbol Curation
- **Detail Sidebar Tab:** Add a **"Call Graph"** tab showing a dynamic tree layout of:
  - *Callers:* Upstream functions that call the selected symbol.
  - *Callees:* Downstream functions called by this symbol.
- **Symbol Curation:**
  - Users can select individual functions in the sidebar or canvas and click "Add Symbol to Context".
  - The Context Workspace registers these symbols (e.g. `src/store.ts#useGraphStore`).
  - When exporting, the prompt compiler reads *only* the specific function line ranges using `read_symbol` instead of fetching full files, minimizing agent token costs.

### 13.3 Live Sync Event Listener
- **Sub-50ms Reload:** When triggered, re-fetch the updated graph data over the IPC bridge and refresh the canvas and explorer tree in real-time, flashing a green status indicator showing `Watching (Synced - Updated)`.

## 14. Futuristic Semantic Knowledge Graph & Explore Tool

This upgrade transitions the repository database into a semantic knowledge graph featuring normalized node/edge vocabularies, edge provenance metadata (heuristics), full-text search, and batch context curation.

### 14.1 Standardized Vocabularies & SQLite Schema
- **Node Kinds (Vocab):** `file`, `module`, `class`, `struct`, `interface`, `trait`, `protocol`, `function`, `method`, `property`, `field`, `variable`, `constant`, `enum`, `enum_member`, `type_alias`, `namespace`, `parameter`, `import`, `export`, `route`, `component`, `other`.
- **Edge Kinds (Vocab):** `contains`, `calls`, `imports`, `exports`, `extends`, `implements`, `references`, `type_of`, `returns`, `instantiates`, `overrides`, `decorates`.
- **Provenance System:** Store `provenance` metadata on edges in the database:
  - `ast`: Directly extracted by AST visitors/regex scanners.
  - `heuristic`: Synthesized at dynamic boundaries (e.g. React context connections, event listeners, callbacks). surfacing the synthesis site.

### 14.2 Full-Text Search (FTS5)
- **FTS5 Virtual Table:** Create an FTS5 virtual table `symbols_fts` inside SQLite:
  `CREATE VIRTUAL TABLE symbols_fts USING fts5(name, file_path, content);`
- **Fuzzy Symbol Matcher:** Implement Tauri/MCP query `search_symbols(query: string)` enabling fast matching of function names and declarations.

### 14.3 The `explore` Tool (Batch Curation)
- **Tool Signature:** `explore(symbols: string[]) -> ExplorePayload`
- **Execution Logic:**
  1. For each symbol in the array, retrieve its line range and source code.
  2. Perform a one-hop walk on the call graph to find callers/callees.
  3. Group matching symbols **by file** to construct a single unified Markdown context payload.
  4. Return the source code along with the visual call paths, giving the agent a highly complete, low-token description of the subsystem in a single turn.

## 15. Semantic Search UI, Heuristic Edge Styles & Batch Explore Exporter

This milestone bridges the SQLite FTS5 search index, edge provenance metadata, and the batch `explore` tool directly into the React visualizer canvas and sidebar.

### 15.1 FTS5 Symbol Fuzzy Search UI
- **Global Search Expansion:** Upgrade the search input component (`TopToolbar.tsx`) to support both file search and symbol-level semantic search.
- **Symbol Mode:** Pressing a modifier or prefixing search (e.g. `sym:foo` or when toggled to "Symbols") invokes the Tauri command `search_symbols({ query })`.
- **Canvas Focus:** Selecting a search result centers the viewport on the parent file node and highlights the target nested symbol node inside it (using spring transitions, <200ms).

### 15.2 Visual Edge Provenance & Heuristics
- **Canvas Styling:** In `EdgeHighlight.tsx`, check the edge's `provenance` metadata:
  - If `ast`, render the standard solid connection.
  - If `heuristic` (synthesized context/callback), render a dashed pink connection line (`stroke-dasharray: 4`).
- **Connection Tooltip:** Hovering over a heuristic edge displays a floating micro-panel showing the `wiring_site` details explaining where the connection was synthesized.

### 15.3 Batch Explore Exporter Integration
- **Zustand & Exporter Connection:** Update the `promptExporter.ts` and Zustand store copy action to call the Tauri `explore` command.
- **Single Payload Assembly:**
  - Pass the array of curated paths and symbols from `contextFiles` and `contextSymbols` directly into `explore()`.
  - Replace the multi-command sequential file loop with a single invocation of the `explore` command, retrieving the fully resolved sub-manifest, calls path graph, and grouped code slices in a single turn.

## 16. Reference Resolution, Web Framework Routing & Dynamic Dispatch Synthesizers

This milestone refactors the query engine to resolve symbol name definitions, parse directory configs, and synthesize edges across dynamic programming boundaries.

### 16.1 Advanced Reference Resolution (Rust Backend)
- **tsconfig.json Path Mapping:** Parse the `compilerOptions.paths` block of local `tsconfig.json` files. When resolving TypeScript import pathways (e.g., `@/utils/math`), map them to the correct local repository folders before checking graph existence.
- **Call-to-Definition Binder:** During the graph construction phase, walk the extracted call nodes. For any call (e.g. `execute()`), match it to its imported declaration symbol and construct an edge `{ from_symbol, to_symbol, kind: 'calls', provenance: 'ast' }` pointing to the exact declaration site in the target file.
- **Inheritance Traversal:** Scan class definitions for `extends` and `implements` and build `extends` / `implements` edges between the class node and parent trait/interface nodes.

### 16.2 Web Framework Routing awareness
- **Route-to-Handler Binding:**
  - Next.js App Router: Parse page/route files (`app/**/page.tsx`, `app/**/route.ts`) and synthesize `references` edges from the route node directly to the page's default export function or API handler function symbol.
  - Python FastAPI/Flask: Map route decorators (e.g., `@app.get("/users")`) directly as a `references` edge from the URL endpoint pattern to the underlying python function symbol.

### 16.3 Dynamic-Dispatch Heuristic Synthesizers
Add a suite of post-index heuristic solvers to search the SQLite database and create `heuristic` edges:
- **EventEmitter Channel Solver:** Match `emitter.emit('channel')` calls to `emitter.on('channel', callback)` registrations, synthesizing an edge from the emit call site directly to the callback function.
- **Interface-to-Implementation Solver:** If a call is made to an interface method (e.g., `Worker.run()`), find all classes implementing the `Worker` interface and synthesize `heuristic` edges from the call site to each concrete `run()` implementation.
- **React JSX Component Child Solver:** Traces JSX component declarations (e.g., `<Button />` or child component trees) and links them to the target default function export using a `references` heuristic edge.
- **React State Render Sync:** Connects React hooks `useState` write actions (`setState`) to the host function component's render execution block.

## 17. Production Release, Packaging & User Handover

This final stage compiles the application for production distribution and establishes the integration guide for developers to hook up their agents.

### 17.1 Tauri Release Compilation
- **Compiler Command:** `npm run tauri build` (compiles optimized production assets, packages the React visualizer, and bundles them into a Windows MSI installer).
- **Target Outputs:**
  - Standalone Executable: `src-tauri/target/release/repo-graph.exe`
  - Installer: `src-tauri/target/release/bundle/msi/repo-graph_*.msi`
  - Standalone MCP Binary: `src-tauri/target/release/mcp_server.exe`

### 17.2 Developer Integration Guide
Create a user manual explaining:
1. **Desktop App Installation:** Running the MSI installer to register the desktop app locally.
2. **MCP Agent Configuration:**
   - How to link the compiled `mcp_server.exe` directly to Claude Desktop, Cursor, and Claude Code.
   - Example configuration blocks demonstrating dynamic workspace sync:
     ```json
     {
       "mcpServers": {
         "repo-graph": {
           "command": "C:/My-pro/project-map/src-tauri/target/release/mcp_server.exe",
           "args": ["auto"]
         }
       }
     }
     ```
3. **Prompt Curation Flow:** Step-by-step instructions on utilizing the Visual Context Workspace, adding specific file and symbol sub-nodes, and copying the optimized 90%+ context-saved markdown prompt directly to the agent clipboard.

## 18. Watcher Staleness Warnings & Catch-up Reconciliation

To prevent agents from receiving stale code contexts during active editing, the MCP server tracks pending file updates and adds context warnings to tool responses.

### 18.1 Thread-Safe Pending Sync Tracking
- **State Map:** The Rust file watcher (`watcher.rs`) maintains a global, thread-safe `Arc<Mutex<HashMap<PathBuf, Instant>>>` tracking all file edit events currently waiting for the indexer debounce timer.
- **Environment Variable Debounce:** The default debounce period is `2000 ms`. It can be overridden via `CODEGRAPH_WATCH_DEBOUNCE_MS`, clamped to `[100ms, 60000ms]`.
- **Deregistration:** Once the indexer re-runs for a file and updates the database, the file path is removed from the pending sync map.

### 18.2 Per-File Staleness Warning Banners
When the agent invokes an MCP tool (e.g., `get_manifest` or `read_file`):
- **Stale Check:** The server cross-references the files about to be returned with the pending sync map.
- **Warning Header:** If any returned file is pending sync, prepend a warning banner to the response:
  ```markdown
  ⚠️ Some files referenced below were edited since the last index sync —
  their graph entries may be stale:
    - <relative_path> (edited <X>ms ago, pending sync)
  For accurate content of those specific files, Read them directly.
  The rest of this response is fresh.
  ```
- **Stale Footer:** If there are pending files elsewhere in the project that are NOT in the response, append a footnote at the bottom:
  `* (Note: N file(s) elsewhere in this project are pending index sync but were not referenced above: ...)`

### 18.3 Connect-Time Catch-up Reconciliation
- **Stat Reconciliation:** On client connection to the MCP server, the server compares the filesystem `mtime` and size properties of all source files against the SQLite database entries.
- **Immediate Catch-up:** Any file edited since the last database write is added to a catch-up indexing queue and parsed immediately, ensuring the graph is fresh before the first query is answered.

### 18.4 `codegraph_status` MCP Tool
- **Status Tool:** Expose the `codegraph_status()` tool returning a markdown block detailing:
  - Active project root path.
  - Sync state (Synced / Pending).
  - List of currently pending files and the elapsed time since they were modified.

## 19. Extended Multi-Language Web Framework Router Parsers

To cover major software ecosystems, the parsing engine supports modern file-based routers, serverless layouts, and enterprise frameworks. The extracted routing nodes link directly to their handler function or controller class symbols.

### 19.1 JavaScript/TypeScript Meta-Frameworks
- **Next.js App & Pages Router:** Parse `app/**/page.tsx` and `pages/api/` routing pathways. Resolve dynamic `[id]` segments and catch-all `[...slug]` patterns. Connect routes to named exports (`GET`, `POST`, `PATCH`) in `route.ts`.
- **Remix:** Parse `routes/` directory structures andflat-file layouts (`routes/posts.$id.tsx`). Map handler references to the named exports `loader` and `action`.
- **Hono & Koa:** Extract route paths from `app.get()`, `app.post()`, `router.get()`, and `app.route()` registrations, binding them to local handler callbacks or imported components.
- **AdonisJS:** Parse `Route.get('/path', 'Controller.method')` mappings and extract target controller symbols.

### 19.2 Python Routers
- **Sanic & Litestar:** Parse `@app.route()`, `@get()`, and `@post()` decorators. Trace class-based views inheriting from `HTTPMethodView`.
- **Tornado:** Parse array tuples in `tornado.web.Application` matching regex URLs directly to target `RequestHandler` subclasses.

### 19.3 Go Routers
- **Echo & Fiber:** Scan for `e.GET()`, `app.Get()`, and `.Group()` sub-router calls, tracing endpoint mappings to handler functions.
- **Beego:** Parse Beego comment routing decorators `// @router /path [get]` above controller methods.

### 19.4 Rust Routers
- **Poem & Axum:** Scan for routing builders: `Route::new().at("/x", get(handler))` or `Router::new().route("/x", get(handler))`.

### 19.5 Java / Kotlin Enterprise Frameworks
- **Micronaut & Quarkus:** Scan for controller class decorators `@Controller("/path")` and HTTP method annotations `@Get()`, `@Post()`, JAX-RS `@Path`, and Reactive `@Route`.
- **Ktor (Kotlin):** Parse nested `routing { get("/x") { ... } }` DSL block paths and link them to handler scopes.

### 19.6 PHP & C# Minimal APIs
- **Symfony & Slim:** Parse PHP `#[Route('/path')]` attributes or Slim `$app->get()` routing declarations.
- **C# Minimal APIs:** Scan for endpoint map routes: `app.MapGet("/x", handler)`, `app.MapPost(...)`, and `app.MapMethods()`.

## 20. Framework Route Styles, Component Highlights & Routes Explorer

This milestone updates the frontend visualizer to distinguish API routes and UI components on the canvas, and displays a dedicated list of routes in the Left Sidebar.

### 20.1 Semantic Symbol Node Styling
Customize node styling in `CustomSymbolNode.tsx` based on `data.kind`:
- **API Route Nodes (`kind === 'route'`):**
  - Styled with an emerald green theme: `border-emerald-500/40 bg-[#14261e] text-emerald-300 hover:border-emerald-400`.
  - Display an uppercase HTTP method prefix (e.g. `GET`, `POST`, `DEL`, `PUT` parsed from prefix) and green handle connections.
- **UI Component Nodes (`kind === 'component'`):**
  - Styled with a cyan React-like theme: `border-cyan-500/40 bg-[#12222a] text-cyan-300 hover:border-cyan-400`.
  - Display a `CMP` kind prefix and cyan handle connections.
- **Default Nodes:** Retain standard purple/grey styles.

### 20.2 Distinct Connection Path Highlights
Customize edge coloring in `EdgeHighlight.tsx` based on relation types:
- **API Binding Edges:** If linking a `route` symbol node to its handler function, render a solid emerald green path with a tooltip showing endpoint details.
- **Component Children / Nesting:** If linking components or React hooks, render a dashed cyan path.

  - Automatically selects the route symbol node and opens the **"Call Graph"** tab in the Detail Sidebar, allowing the user to trace execution flows immediately.

## 21. Context Cost & Token Savings Benchmarking Suite

To prove the efficiency gains of Repo Graph, the repository includes an automated benchmarking script that calculates token ingestion, API call round-trips, and financial cost comparisons.

### 21.1 Benchmark Mathematical Formulation
- **Token Estimation:** 1 token $\approx$ 3.7 characters (standard estimate for source code).
- **Baseline Ingest Cost ($C_{base}$):** The token cost of crawling and loading target files in full.
  \[C_{base} = \sum_{f \in F_{crawl}} \text{tokens}(f)\]
- **Repo Graph Ingest Cost ($C_{graph}$):** The token cost of the scoped manifest plus the targeted symbol line ranges.
  \[C_{graph} = \text{tokens}(\text{manifest}) + \sum_{s \in S_{curated}} \text{tokens}(\text{slice}(s))\]
- **Savings Ratio:**
  \[\text{Savings \%} = 100 \times \left(1 - \frac{C_{graph}}{C_{base}}\right)\]

### 21.2 Automated Benchmark Script (`scripts/benchmark.js`)
Create a Node.js script that:
1. Simulates a standard agent resolving a multi-file dependency task (e.g. "Find where the open_in_editor Tauri command is declared, read its implementation, and find what calls it").
2. **Arm A (No-Repo Graph / Baseline):** Simulates the file crawl:
   - Walks files, calls grep, reads `main.rs` (506 lines), `db.rs` (624 lines), and `store.ts` (345 lines) in full.
   - Calculates total character weight and token count.
3. **Arm B (With Repo Graph):** Simulates the query path:
   - Reads the scoped subgraph manifest returned by `get_manifest()`.
   - Calls `explore()` to pull the exact line ranges for `open_in_editor` (38 lines) and its call path.
   - Calculates the character weight and token count.
4. **Output Report:** Logs a Markdown table comparing:
   - File reads (count)
   - Tool calls / round-trips (count)
   - Ingested character weight & token count
   - Cost in USD (assuming standard LLM prices, e.g. \$3.00 / million input tokens)
   - Calculated Savings % (Target: > 90% savings)

## 22. Gemini Antigravity Agent Configuration

To integrate the Repo Graph MCP server into the Gemini Antigravity agent environment, configure its local app data config registry.

### 22.1 Configuration File Integration
- **Config Path:** `C:\Users\nmahe\.gemini\antigravity\mcp_config.json`
- **Format:**
  ```json
  {
    "mcpServers": {
      "repo-graph": {
        "command": "C:/My-pro/project-map/src-tauri/target/debug/mcp_server.exe",
        "args": ["C:/My-pro/project-map"]
      }
    }
  }
  ```
- **Loader Action:** Upon next startup of the Antigravity session, the system automatically parses `mcp_config.json`, starts the registered `repo-graph` background process, and registers its tools under the `repo-graph/*` namespace.

## 23. SQLite-Only Frontend Pipeline & Visual Agent Telemetry

This upgrade transitions the application into a pure database-driven architecture, eliminating intermediate flat files for O(1) watch updates, and hooks agent tool usage telemetry into the visualizer canvas.

### 23.1 Deprecation of JSON Graph Cache
- **The Gap:** The watcher currently triggers a full index walk (`index_repo`) on every save to write `.repograph/graph.json`. For large codebases, this blocks CPU cycles and disk writes.
- **SQLite-Only Stream:** Both `read_graph` and `index_and_load_graph` Tauri commands are refactored to read nodes and edges directly from `.repograph/graph.db` using optimized SELECT queries, deprecating `.repograph/graph.json` entirely.
- **O(1) Incremental Watches:** When a file is modified, the watcher parses ONLY that file and commits the updates to SQLite. The visualizer re-fetches only the modified delta or queries the DB directly, completing syncs in `< 10 ms` regardless of codebase scale.

### 23.2 Visual Agent Telemetry overlay
- **Tool Event Bridge:** The MCP server (`mcp_server.rs`) and database write transactions emit global Tauri events (e.g. `agent_query_event`) whenever an agent executes `explore()`, `read_symbol()`, or `search_symbols()`.
  - It renders an **"Agent Activity Feed"** logs overlay in the corner of the canvas showing a real-time trail of the agent's research actions (e.g., *"Agent explored print_report in main.rs (120ms ago)"*).

## 24. Visual Project Hub & Quick Switcher Dashboard

To support multi-project management and deliver a premium, IDE-like entry experience, the application includes a **Visual Project Hub** dashboard and a **Quick Switcher** workspace utility.

### 24.1 Recent Projects Config Registry
- **Config Store:** Maintain a JSON list of recent projects at:
  `C:\Users\nmahe\.gemini\antigravity\recent_projects.json`
- **Format:**
  ```json
  {
    "projects": [
      {
        "path": "C:/My-pro/project-map",
        "last_opened": "2026-07-19T12:00:00Z",
        "stats": { "files": 112, "symbols": 450, "language": "rust" }
      }
    ]
  }
  ```
- **Tauri Registry Hooks:** Whenever a project is successfully opened, loaded, or synced via Tauri, the backend appends/updates the project details in `recent_projects.json`.

### 24.2 Visual Project Hub Dashboard (Landing Splash Page)
- **Interactive Landing Page:** If `activeProjectRoot` is null, the app displays a dark landing dashboard ("Repo Graph Hub"):
  - **Quick Start:** A prominent call-to-action button to open a new folder using the file dialog.
  - **Recent Projects List:** Interactive cards for each recent project from `recent_projects.json` showing file counts, symbol density, primary language, and relative time since last opened.
  - **Instant Switch:** Clicking a card instantly sets the root path, loads the database graph, and bypasses the file picker dialog.
  - **Quick Configs:** Embedded settings sliders to adjust the watch debounce time (`CODEGRAPH_WATCH_DEBOUNCE_MS`).

- **Header Switcher:** In the Left Sidebar header next to the active project title, display a dropdown switcher listing the top 5 recent project paths.
- **O(1) Switching:** Selecting a path triggers the Zustand store to disconnect from the current watcher, update `activeProjectRoot`, and load the target SQLite graph data within 10 ms.

## 25. Single-Tool Default Listing & Environment Filter Protocols

To prevent LLM tool selection errors and steer agents toward compound, token-efficient queries, the MCP server restricts its default visible toolset and implements allowlist env filters.

### 25.1 Tool Registry Refactor
All tools are renamed to adopt the `repograph_` namespace:
- `repograph_explore` (compound symbol explore/references lookup)
- `repograph_node` (reads a symbol's source and edges, or full file)
- `repograph_search` (symbol name matching)
- `repograph_callers` (finds symbol callers)
- `repograph_callees` (finds symbol callees)
- `repograph_impact` (blast radius analysis)
- `repograph_files` (repository file structure manifest)
- `repograph_status` (index health and pending sync statistics)

### 25.2 Single-Tool Default & Environment Filters
- **Default Behavior:** In the `tools/list` JSON-RPC response, the MCP server returns ONLY `repograph_explore` by default.
- **Allowlist Filtering:** Parse `REPOGRAPH_MCP_TOOLS` as a comma-separated list of short names (e.g. `explore`, `node`, `search`).
  - If the env variable is present, only return tools in the allowlist.
  - If the env variable is absent, return only `repograph_explore`.
- **Invocation Support:** The MCP server must support execution calls (`tools/call`) for ALL 8 registered tools at all times, even if they are currently filtered out of the list response.

### 25.3 Payload Format v1.1.0 — `signature_only` & `compact_edges`

`repograph_explore` takes two optional booleans that change the *shape* of the response, not what it can find. Both are additive; omitting them reproduces the pre-v1.1.0 output.

| Parameter | Default | Effect |
| :--- | :--- | :--- |
| `signature_only` | `false` | Returns each symbol's declaration head instead of its body — first line through the line that opens the body (`{`, `:`, `=>`, `;`), capped at 6 lines. `start_line`/`end_line` still report the symbol's true span, so an agent can see how much body it skipped and fetch it. |
| `compact_edges` | **value of `signature_only`** | Serializes `paths` as arrow strings `from -kind-> to` instead of `{"from_symbol": …, "to_symbol": …, "kind": …}`, removing 44 chars of repeated keys per edge. |

```jsonc
// architecture question — 1,473 chars
{"name": "repograph_explore",
 "arguments": {"symbols": ["useGraphStore"], "signature_only": true}}

// about to edit the implementation — fetch the body
{"name": "repograph_explore", "arguments": {"symbols": ["useGraphStore"]}}
```

Measured on `useGraphStore`: 12,679 chars (full body) → 2,085 (`signature_only`) → **1,473** (both), i.e. **98.56%** against a 27,620-token baseline. Full arc and caveats in `docs/BENCHMARK_REPORT.md` §2.

**Two version fields, deliberately distinct:**

| Field | Type | Value | Meaning |
| :--- | :--- | :--- | :--- |
| `MANIFEST_SCHEMA_VERSION` (`manifest.rs`) | semver string | `1.1.0` | Agent-facing payload format. Rendered in the manifest footer (`## Agent Instructions (payload format v1.1.0)`) — the only place an agent can read it. |
| `Graph::schema_version` (`graph.rs`) | `u32` | `1` | On-disk `.repograph/graph.json` cache layout. **Unchanged** — bumping it would fail to deserialize every existing cache and contradict `src/types.ts` (`schema_version: number`). |

### 25.4 Semantic Symbol Kinds & Badges

`parsers/schema.rs` and `parsers/state.rs` re-tag symbols with semantic kinds, surfaced identically on the canvas (`CustomSymbolNode.tsx`) and in the detail sidebar (`DetailSidebar.tsx` `KIND_BADGES`):

| Kind | Badge | Colour | Detected from |
| :--- | :--- | :--- | :--- |
| `database_schema` | `DB` | indigo `#17162e` / `indigo-300` | Prisma `model`, TypeORM `@Entity`, Drizzle `pgTable`, Sequelize `.define`, Mongoose, SQLAlchemy/Django models, Diesel `table!`/`Queryable`, SeaORM, GORM structs, JPA `@Entity`, EF `DbSet<T>`, Doctrine, Eloquent |
| `event_channel` | `EVT` | rose `#281519` / `rose-300` | EventEmitter `.emit`/`.on`, RxJS `Subject`, Redis `publish`/`subscribe`, Kafka topics |
| `state_store` | `STR` | amber `#261d10` / `amber-300` | Zustand, Redux, React Context, Pinia, Vuex, MobX, Recoil/Jotai, Celery, Rust statics/channels |
| `route` | `API` | emerald `#0F241B` / `emerald-300` | HTTP frameworks (§19), GraphQL SDL `type Query`/`Mutation`, tRPC `publicProcedure.query()` |
| `component` | `CMP` | cyan `#0F2028` / `cyan-300` | JSX/TSX component declarations |

**Every rule is import-gated.** A bare `create(`, `model(`, `Entity` or `.emit(` is far too generic to claim without the framework being referenced in the same file, and the scanners additionally skip Rust `r#"…"#` blocks and Python docstrings — a parser's own test fixtures otherwise get indexed as real models. Unlisted kinds fall back to a 3-letter truncation of the kind string.

### 25.5 Agent Initialization & Installer Markers
- **MCP Initialize response:** Add descriptive usage text to the `instructions` field in the MCP `initialize` output, explaining to the agent that `repograph_explore` is the primary and most token-efficient search/read tool.
- **Marker-Fenced Rules Installer:** The server CLI includes an `install-rules` command that appends a fenced instruction block to the workspace rules file (`my-agent/RULES.md` and `.agents/AGENTS.md`) steering agents to use the CLI equivalent of `repograph_explore` for context searching.

---

## 26. Global Context Engineering Prompt Architecture (CEPA)

To help global users and AI agents minimize token usage, the project defines a standardized **Context Engineering Prompt Architecture (CEPA)**.

### 26.1 The Seven-Piece Context Stack Mapping
Agents utilizing the `repograph` MCP server must structure their short-term memory partition:
1. **Instructions:** Guardrails preventing blind glob/grep calls; prioritizing `repograph_explore`.
2. **User Input:** Task description.
3. **Retrieved Facts:** Verbatim symbol signatures and edge maps returned by MCP.
4. **Tools:** Schema declarations for `repograph_explore`, `repograph_node`, `repograph_search`, etc.
5. **Short-term Notes:** Active checklists maintained in `my-agent/memory/runtime/context.md`.
6. **Long-term Memory:** Codebase rules and style constraints.
7. **Output Format:** Strict markdown diffs or JSON payloads.

### 26.2 Three-Step Discovery Sequence
To minimize token firing on global projects:
- **Step 1: Orient (Structure):** Call `repograph_status` and `repograph_files` to map the workspace layout.
- **Step 2: Target (Search):** Call `repograph_search(query)` (utilizing the split camelCase tokenizer) to find relevant symbol references.
- **Step 3: Explore Leanly (Explore):** Always call `repograph_explore(symbols, signature_only: true)` first to examine declarations and caller edges. Only load the implementation body (`signature_only: false`) when modification or deep logic auditing is required.

---

## 27. Empirical Retrieval Semantics & Query Hardening

Findings from live stdio probing of all 8 tools against this workspace. These are observed behaviours, not design intentions — treat them as the contract agents must actually code against.

### 26.1 Token Sizing Rule of Thumb

`repograph_explore` is not universally cheaper than reading a file. Savings scale with how little of the file you discard.

| Symbol-to-file ratio | Preferred tool | Rationale |
| :--- | :--- | :--- |
| Small symbol in a large file (<30%) | `repograph_explore` | Slicing discards the bulk — savings approach the benchmark figures |
| Call graph needed anyway | `repograph_explore` | Code + edges in one round-trip |
| Symbol dominates its file (>70%) | `repograph_node(path)` | `explore` appends `paths` overhead on near-identical code |
| Repo-wide orientation | `repograph_files` | Manifest substitution is the real source of >90% savings |

Measured counter-case: `repograph_explore(["buildFlow"])` = 7,834 chars (~2,117 tok) versus a full read of `src/lib/layout.ts` at 5,959 chars (~1,611 tok) — **1.31× more expensive**, because `buildFlow` is 75% of its file. See `docs/BENCHMARK_REPORT.md` §5.

### 26.2 Path Disambiguation Requirement

`repograph_callers`, `repograph_callees`, and `repograph_impact` **must** receive both `path` and `symbol`. Bare-name lookups miss silently:

```
repograph_callers(symbol: "main")                                  -> "No callers found for symbol 'main'."
repograph_callers(path: "src-tauri/src/main.rs", symbol: "main")   -> src-tauri/src/main.rs#src-tauri/src/main.rs (file)
```

`repograph_explore` does resolve bare names (`["buildFlow"]` works), but `'path#symbol'` references remain safer for names repeated across files (`main`, `default`, `App`).

### 26.3 Silent Miss Handling

Empty results are **non-matches, not server errors**. `repograph_explore` returns `{"files":[],"paths":[]}` and `repograph_search` returns `[]` for symbols that do not exist — indistinguishable from a typo. Agents must refine the query (fall back to `repograph_search`, then `repograph_files`) rather than reporting a system failure or concluding the symbol is absent.

### 26.4 Response Format Is Not Uniform

Only `repograph_explore` (JSON object) and `repograph_search` (JSON array) return JSON. `repograph_files` and `repograph_status` return markdown; `repograph_node` returns raw source with no wrapper; `repograph_callers`/`callees`/`impact` return plain-text lists. Never blanket-`JSON.parse` a tool result.

### 26.5 Known Result-Quality Caveats

- `repograph_callees` includes in-function local variable declarations alongside real calls (24 of 30 edges for `buildFlow`). Filter for `(function)` / `(component)` kinds.
- Entries shaped `path#path` (kind `file`) are the file node itself, not a symbol.

### 26.6 Cache Bloat & Startup Hang

Observed: `.repograph/graph.db` at **664 MB for 136 files** (vs 448 KB for `graph.json`), causing `mcp_server` to **hang on startup reconciliation for >120 s with no output**.

Root cause (fixed): the watcher's `is_supported_file` had a private denylist covering only `.git`, `node_modules`, and `.repograph` — omitting `dist`, `build`, `target`, `out`, `.next`, and `coverage` from `walker::SKIP_DIRS`. Every `npm run build` therefore made the watcher re-parse `dist/assets/index-<hash>.js`; because minified bundles are a few enormous lines, each of the thousands of extracted single-letter symbols stored a ~190 KB slice in `symbols_fts`. `dbstat` confirmed `symbols_fts_content` alone at 470 MB across 4,374 rows. Row counts were *normal* — this was never a duplicate-row problem, which is why a `COUNT(*)`-based test could not reproduce it.

The watcher now delegates to `walker::SKIP_DIRS` + `walker::language_for` + `parsers::extractor_for`, so it can never index something a full re-index would skip. A fresh index is **0.37 MB / 139 files / 343 ms**. Caches built before the fix must be deleted manually and re-indexed.

---

## 28. Autonomous Agent Scaffolding (.myrepograph-agent/)

When a workspace is browsed or indexed in Repo Graph, the backend automatically verifies whether the `.myrepograph-agent/` scaffolding directory exists at the workspace root. If absent, it autonomously creates the directory structure and default context engineering configuration files:

```
.myrepograph-agent/
├── agents/
│   └── fact-checker/
│       ├── agent.yaml
│       ├── DUTIES.md
│       └── SOUL.md
├── compliance/
├── config/
├── examples/
├── hooks/
│   ├── bootstrap.md
│   └── teardown.md
├── knowledge/
├── memory/
│   └── runtime/
│       ├── context.md
│       └── dailylog.md
├── skills/
│   └── code-review/
│       ├── review.sh
│       └── SKILL.md
├── tools/
├── workflows/
├── agent.yaml
├── AGENTS.md
├── DUTIES.md
├── RULES.md
└── SOUL.md
```

This guarantees that any project opened by global users immediately contains context-engineered agent rules (`RULES.md`), memory scratchpads (`memory/runtime/context.md`), duties (`DUTIES.md`), and agent hooks (`hooks/bootstrap.md`), steering all incoming AI agents to consume minimal context tokens.






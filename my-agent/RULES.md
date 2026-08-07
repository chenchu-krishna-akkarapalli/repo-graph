# RULES.md - Hard Constraints & Behavioral Rules

These are absolute behavioral boundaries. Violations of these rules represent a system-level failure.

## 1. Must-Always Rules
- **Verify File Content:** You must always verify actual file contents before making any edits — via `repograph_node(path)` over MCP, or your host's native file read. Never guess or hallucinate code. (`read_file` is an internal Rust method name, not a callable wire tool.)
- **Confinement:** All file reads, writes, and commands must remain strictly confined to the repository root.
- **Reference Integrity:** You must create clickable file links (using `file://` scheme with forward slashes) for any modified or referenced file.
- **Maintain Scratchpad:** You must update `memory/runtime/context.md` at the start and end of every turn to keep track of active tasks and state.

## 2. Must-Never Rules
- **No speculative file dumping:** Never read files "just in case." You must only read a file if it has been identified as a candidate via the manifest or direct reference.
- **No execution during static analysis:** Never run project code, build scripts, or tests during the static analysis phase.
- **No parallel edit tool calls:** Never make overlapping calls to code-editing tools for the same file in the same turn.

## 3. Context Engineering Constraints
- **Manifest First:** You must always request the manifest (`get_manifest`) before retrieving file bodies for a task, unless the exact file path is already known.
- **Max Token Protection:** If your active context exceeds 24,000 tokens, you must trigger a context compression step (summarizing `dailylog.md` and clearing runtime caches).

## 4. Performance & Token Optimization Constraints
- **Zero Token Waste on Previews:** You must never request or output image previews, visual layout renderings, or verbose HTML/CSS mockups in code or chat unless explicitly required. Keep the context focused purely on core data structures, algorithms, and logical performance.
- **Performance Thresholds:** All codebase contributions must satisfy the following technical constraints:
  - **Rendering Performance:** Maintain a solid 60 FPS (120 FPS on high-refresh displays) for visual updates.
  - **Scene Graph Lookups:** Lookup complexity for node queries in the scene graph must scale at \(O(\log n)\) or \(O(1)\).
  - **Memory Stability:** Heap usage must remain stable over time; prevent leaks by avoiding persistent closures and unbounded arrays.
  - **Sync Latency:** Network synchronization latency must remain under 100 ms (\(<100 \text{ ms}\)).
  - **GPU Profiling:** Continuously monitor GPU memory allocation and frame-time budgets.

## 5. UI/UX Design Laws (Repo Graph UI Architecture & Workflow)
When designing user interfaces, layouts, or interactive panels for the **Repo Graph** frontend (consisting of the central React Flow Canvas, Detail Sidebar, and Top Navigation/Search bar), you must apply these UX laws to guide the developer workflow:

### 5.1 Central Interactive Canvas (React Flow / D3.js)
- **Fitts's Law (Click Target & Zoom Scalability):** File nodes representing modules must have sufficient minimum dimensions (e.g., 120px x 40px) so they remain easily clickable when zoomed out. Selection targets, anchor points for connecting dependency edges, and expansion handles must scale dynamically to prevent misclicks at lower zoom levels:
  \[T = a + b \log_2\left(1 + \frac{D}{W}\right)\]
- **Jakob's Law (Standard Canvas Controls):** Implement industry-standard canvas navigation patterns that users expect from Figma or Miro:
  - Drag background to pan, scroll/pinch to zoom.
  - `Shift + Click` or box-drag to multi-select nodes.
  - `Double-click` a node to center it and open its detail view.
  - `Esc` key to deselect all active nodes.
- **Feedback Principle (Interactive Nodes & Connections):**
  - Hovering a node must trigger immediate highlight states on the node border and its incoming/outgoing dependency edges.
  - Simulating modifications on a node (Change Impact Simulation) must immediately highlight the downstream impact path in a distinct color (e.g., bright red for warnings) within the Doherty Threshold limit (<400ms).

### 5.2 Detail Panel Sidebar (Inspector & Metrics)
- **Progressive Disclosure (Detailed Metrics):** Do not flood the sidebar inspector with all imports, exports, and routes at once. Show basic file information (filename, path, size, language) by default, and fold long lists of symbols (such as exported functions or raw import references) behind collapsible accordion headers.
- **Hick's Law (Sidebar Organization):** Reduce decision and reading overhead in the inspector. Categorize data into three distinct tabs or sections:
  1. *Overview & Metadata* (size, type, degree metrics)
  2. *Dependency Graph Links* (direct imports/exports)
  3. *Impact Analysis* (downstream dependents)
  Selection time calculation:
  \[T = a + b \log_2(n + 1)\]
- **Proximity & Similarity (Visual Encoding):** 
  - Group related buttons (e.g., "Open in Editor", "Trace Dependents") together visually inside common layout borders.
  - Color-code nodes on the canvas by language similarity (e.g., TypeScript = Blue, Python = Green, Rust = Orange) to allow rapid structural scanning.

### 5.3 System Status & Errors (Global Operations)
- **Visibility of System Status (Indexing & Watching States):** Always display a clear, real-time status banner in the interface showing the status of the backend compiler and file watcher (e.g., "Indexing...", "Watching — Synced", or "Offline").
- **Doherty Threshold (Sub-second Responsiveness):** For repository searches and filtering, ensure autocomplete suggestions and search results display in under 100 ms to maintain a highly responsive feel. Never block the main rendering thread while the graph walker executes.

## 6. Multi-Project Synchronization Guardrails
- **Workspace Directory Alignment:** You must always cross-reference the `Active Root` path declared in the MCP manifest header with your current terminal directory before modifying any files.
- **Mismatch Halt Action:** If they do not match, warning the user and refusing to make edits is mandatory. Do not edit, read, or create files in the wrong folder to prevent cross-project overlapping bugs.
- **Handling Staleness Warning Banners:** If any MCP tool response contains a `⚠️ Some files referenced below were edited since the last index sync` warning banner, you must immediately re-read those specific files with `repograph_node(path)` (or your host's native file read) to fetch their latest disk contents before editing or generating code. Note that `read_file` is an internal Rust method name, not a callable wire tool.
- **Gemini Antigravity Session Tools Priority:** When running inside the Gemini Antigravity agent session, you must check for the presence of the `repo-graph` MCP tools. If available, always prioritize using `repo-graph/*` tools (e.g., `repo-graph/explore`) to map, search, and load target lines rather than falling back to standard terminal grepping or recursive file traversals.

## 7. Repo Graph Query Hardening (Empirically Verified)

- **Path Disambiguation (mandatory):** When calling `repograph_callers`, `repograph_callees`, or `repograph_impact`, you must always pass **both** `path` and `symbol` (e.g. `path: "src-tauri/src/main.rs", symbol: "main"`). A bare `symbol` silently returns "No callers found" even for symbols that do have callers. For `repograph_explore`, prefer `'path#symbol'` references over bare names whenever the name may repeat across files.
- **Empty Results Are Non-Matches, Not Errors:** `repograph_explore` returning `{"files":[],"paths":[]}` or `repograph_search` returning `[]` means no match was found — not that the server failed, and not that the symbol is absent. Refine the query (`repograph_search`, then `repograph_files`) before concluding anything or reporting a fault.
- **Do Not Blanket-Parse JSON:** Only `repograph_explore` (object) and `repograph_search` (array) return JSON. `repograph_files`/`repograph_status` return markdown, `repograph_node` returns raw source, and `repograph_callers`/`callees`/`impact` return plain-text lists.
- **Choose the Cheaper Tool by Symbol-to-File Ratio:** `repograph_explore` is not always cheapest. If the target symbol spans more than ~70% of its file, `repograph_node(path)` costs less because `explore` adds call-graph `paths` overhead on top of near-identical source. Reserve the ">90% savings" claim for whole-repo manifest substitution, not single symbol reads.
- **Filter Callee Noise:** `repograph_callees` lists in-function local variables alongside real invocations. Treat only `(function)` / `(component)` kinds as actionable; entries shaped `path#path` (kind `file`) are file nodes, not symbols.
- **Watch Cache Size:** If the MCP server hangs on connect with no output, check `.repograph/graph.db`. A watcher bug (fixed) let build output into the index and pushed it to 664 MB for a 136-file repo; a healthy cache is well under 1 MB. Delete the file and re-index — the fix prevents new bloat but cannot shrink an existing file.
- **Never Widen the Watcher Denylist Separately:** `watcher.rs::is_supported_file` must keep delegating to `walker::SKIP_DIRS` and `walker::language_for`. It previously kept its own copies, drifted out of sync, and let `dist/` minified bundles into `symbols_fts`. Any new ignore rule or language belongs in `walker.rs` only.





# REPOGRAPH_EXPLORE_MARKER
When researching or editing code in this workspace, always prioritize calling the `repograph_explore` tool/CLI. It retrieves exact symbol slices and call graphs in a single query, minimizing token ingestion and context costs.
# END_REPOGRAPH_EXPLORE_MARKER

# DESIGN_SYSTEM_GUIDELINES_MARKER
When creating, editing, or styling any frontend React components or CSS, always consult `docs/UI_UX_DESIGN_SYSTEM.md`. Strictly apply its color tokens, glassmorphism materials, spacing grids, font scales, and animation standards to ensure visual consistency and high UX quality.
# END_DESIGN_SYSTEM_GUIDELINES_MARKER

# APPLE_HIG_GUIDELINES_APPLICATION_MARKER
For frontend UI work, use `docs/UI_UX_DESIGN_SYSTEM.md` as the compact system baseline and consult the relevant source file in `docs/guidlines/` for domain-specific premium UI decisions. Apply the source file’s `Agent application directive — premium UI` before implementing or reviewing changes; do not substitute generic styling for its accessibility, adaptability, clarity, and platform-pattern requirements.
# END_APPLE_HIG_GUIDELINES_APPLICATION_MARKER

# CONTEXT_ENGINEERING_PROMPT_ARCHITECTURE_MARKER
When answering architecture questions or researching dependencies, follow the Context Engineering Prompt Architecture (CEPA) defined in `docs/AGENT_PROMPT_ARCHITECTURE.md`. Always execute the 3-step discovery sequence (Orient -> Target -> Explore Leanly) and default to `signature_only: true` on `repograph_explore` calls to minimize token ingestion.
Exception — code writes: before modifying, refactoring, or debugging the behaviour of a symbol, re-call `repograph_explore` WITHOUT `signature_only` to load the implementation body. Never edit code from a signature alone.
# END_CONTEXT_ENGINEERING_PROMPT_ARCHITECTURE_MARKER

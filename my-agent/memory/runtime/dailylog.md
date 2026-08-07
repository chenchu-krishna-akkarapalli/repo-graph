# Daily Log - 2026-07-19

## Design System Guidelines Consolidation (2026-07-20)
- Read and distilled all 15 direct-reference guideline files in `docs/guidlines/` into `docs/UI_UX_DESIGN_SYSTEM.md`.
- Added implementation-ready semantic theme tokens, a 4px spacing grid, glass material recipes, Inter/JetBrains Mono hierarchy, component standards, 150–300ms motion rules, and WCAG/safe-area/keyboard requirements.
- Appended and verified the exact `DESIGN_SYSTEM_GUIDELINES_MARKER` block in `my-agent/RULES.md` so future agents must consult the master reference for frontend work.

## Guideline Context Engineering Integration (2026-07-20)
- Read all 15 files in `docs/guidlines/` and added an `Agent context engineering` section to every file.
- Each section operationalizes the seven-piece stack and the write/select/compress/isolate framework for that guideline’s own domain, with a narrow expected output format to prevent context bloat.
- Verified every guideline contains the stack plus topic-specific retrieval, scratchpad/compression, and isolation/output guidance.

## Premium Apple HIG Agent Directives (2026-07-20)
- Corrected the context-engineering-only approach by adding a prominent, domain-specific `Agent application directive — premium UI` immediately after the title of every copied Apple HIG guideline.
- The directives make the source actionable for implementation: accessibility release gates, adaptive colour/theme validation, hierarchy and responsive layout, purposeful materials/motion, legible typography, familiar icons, inclusive content, and appropriate window/spatial behaviour.
- Added `APPLE_HIG_GUIDELINES_APPLICATION_MARKER` to `my-agent/RULES.md`, requiring frontend agents to consult the relevant source guideline as well as the compact design-system baseline.

## Tasks Completed
- **Section 14 Implementation (Semantic Knowledge Graph & Explore Tool):**
  - **SQLite Database Schema Migration:** Added FTS5 search index table `symbols_fts` and trigger `symbols_after_delete` inside `db.rs`. Updated the `edges` schema to capture `provenance` (`ast` vs `heuristic`) and `wiring_site` fields.
  - **Backward Compatibility:** Handled existing cache file deserialization issues warning-free using serde aliases (`#[serde(alias = "...")]`) to bridge old `"import"` keys.
  - **React Context Heuristic Synthesis:** Automatically synthesizes a `heuristic` edge from the usage site of a React context (e.g. `useContext(MyContext)`) to the context declaration, highlighting the wiring site.
  - **Normalized Parsers:** Updated symbols extracted across Go, Rust, Python, and Javascript extractors to map to standardized vocab kinds. Classified class/impl nesting for `method` nodes and PascalCase functions as `component` nodes.
  - **Tauri Commands & MCP Tools:** Implemented `search_symbols` and `explore` commands in `main.rs` and `mcp_server.rs`.
- **Section 15 Implementation (Semantic Search, Provenance, & Exporter UI):**
  - **FTS5 Search Dropdown:** Added a debounced FTS5 autocomplete matching list in `TopToolbar.tsx` when the `sym:` prefix is active. On select, it expands the parent file node, centers it, and highlights the symbol.
  - **Visual selected highlight:** Updated `CustomSymbolNode.tsx` to display active selection highlights with a glowing border.
  - **Dashed Heuristic Edges & Hover Tooltips:** Configured `layout.ts` to map symbol connections using custom EdgeHighlight paths. If the edge has `provenance === 'heuristic'`, it is styled as a dashed pink line and displays a foreignObject tooltip displaying its wiring site context annotations on hover.
  - **Batch Explore Prompt Exporter:** Refactored `copyContextPrompt` in `promptExporter.ts` to fetch both curated files and symbols in a single `explore` IPC command, rendering a clean sub-manifest, call graph paths, and grouped sliced source codes.
  - **Search Matches Layout Fix:** Reorganized the file matches count indicator into an inline flexbox badge to prevent vertical layout clipping inside the header.
- **Section 16 Implementation (Advanced Reference Resolution & Synthesizers):**
  - **TSConfig Path Alias Mapping:** Added alias mappings loader `load_tsconfig_aliases` in `graph.rs`. Cleaned block/line comments from `tsconfig.json` safely. Mapped alias keys (e.g., `@/components/*`) to relative folders during graph resolving.
  - **Class Inheritance Extraction:** Updated Javascript extractor to parse class extensions and implemented interface details. Wrote them back as `extends` and `implements` edges in `db.rs`.
  - **Route-to-Handler Binding:** Automatically parsed Next.js and python entry points to URLs and connected them directly to default exports and handler function symbols.
  - **Dynamic Dispatch Heuristics:**
    - *Interface-to-Implementation Solver:* Join-query mapping interface method call sites to concrete implementation methods.
    - *JSX Component Child Solver:* Tags components dynamically as component references.
    - *EventEmitter Channel Solver:* Scans emitter calls (`.emit`) and callback receivers (`.on`) to map callback handlers.
    - *React State Render Sync:* Synthesized hook write actions (`setState`) self-loop dependencies.
- **Visualizer Symbol Hydration & Blinking Fix:**
  - **get_all_symbols Query:** Implemented retrieval method in `db.rs` to group symbols by their parent file path while ignoring the file-level self-symbol node (preventing recursive inner file nodes rendering).
  - **Command Hydration Seam:** Linked hydration call inside both `read_graph` and `index_and_load_graph` Tauri commands inside `main.rs` to populate `Node.symbols` vectors dynamically on startup.
  - **Hover Blinking Bug:** Updated `onNodeMouseEnter` callback in `src/components/GraphCanvas.tsx` to resolve hover states to `node.parentNode` when cursor is over internal symbol nodes, maintaining visual stability.
- **Vite Watcher & Reload Loop Fix:**
  - **Exclusion Filters:** Configured `server.watch.ignored` in `vite.config.ts` to explicitly exclude internal DB cache directories (`.repograph/**`), project workspace registry metadata (`active_project.json`), and local target compile artifacts.
- **Section 18 Implementation (Watcher Staleness Warnings & Catch-up Sync):**
  - **Global Pending sync map:** Added a thread-safe static `PENDING_CHANGES` map inside `watcher.rs` to register file edit events and clear them after the debounced indexing process finishes.
  - **Clamped Debounce Timing:** Read `CODEGRAPH_WATCH_DEBOUNCE_MS` from env variables and clamped it to `[100ms, 60000ms]`, defaulting to `2000ms`.
  - **Stat-Based Startup Reconciliation:** Updated the SQLite database schema for the `files` table to track the files' modified timestamp `mtime`. Created the reconciliation checker `reconcile_repo_startup` in `db.rs` to scan, compare, and catch up modified files upon client connection.
  - **Staleness Warning Banners & Footnotes:** Enabled `check_staleness` inside `mcp_server.rs` to check the pending map, prepending warnings and appending pending footwear counts to tool outputs for stale files.
  - **codegraph_status MCP Tool:** Exposed and registered `codegraph_status` in `mcp_server.rs` which returns root path, connection status, and list of pending files.
- **Agent Integration & Verification:**
  - **Claude Desktop Config:** Wrote the MCP configuration block successfully to `claude_desktop_config.json`.
  - **Cost Math & Savings Audit:** Completed mock agent session trace, calculating a **96.24%** token context savings compared to full codebase ingestion.
  - **Gemini Antigravity Integration:** Created `C:\Users\nmahe\.gemini\antigravity\mcp_config.json` with the `repo-graph` MCP server registry block and compiled the debug target `mcp_server.exe` successfully.
- **E2E Verification:**
  - Ran cargo tests successfully (26/26 tests passed).
  - Executed frontend TypeScript compile (`npm run build`) successfully with no errors or warnings.
- **Zustand & React Hook Ordering Bug Fix:**
  - Relocated hook definitions `agentActivity` and `isTelemetryExpanded` to the top of the `Canvas` component in `src/components/GraphCanvas.tsx` to follow the React Rules of Hooks.
  - **Section 24 Implementation (Project Hub & Switcher Dashboard):**
  - **Tauri Recent Projects Config Registry:**
    - Implemented a thread-safe static `RECENT_PROJECTS_MUTEX` in `main.rs` to protect reading/writing `C:\Users\nmahe\.gemini\antigravity\recent_projects.json`.
    - Created `get_recent_projects` and `add_recent_project` Tauri commands.
    - Wired automatic registry logging into `read_graph` and `index_and_load_graph` commands, capturing node counts, symbol density, and primary language stats on workspace loads.
    - Added `set_watch_debounce` command to update the watch debounce duration dynamically at runtime.
  - **Visual Landing Dashboard (`ProjectHubDashboard.tsx`):**
    - Built a premium landing dashboard featuring custom grid backdrops, relative time formatting (e.g. "2 hours ago"), stats badges, and loading transition animations.
    - Embedded settings sliders to configure watch debounce times.
  - **Quick Switcher Dropdown (`LeftSidebar.tsx`):**
    - Rendered a chevron toggle next to the active project folder name.
    - Displays the top 5 recent workspaces for O(1) switching, triggering `selectProject()` immediately on selection.
  - **Layout Mounting Integration (`App.tsx`):**
    - Conditionally mounts `ProjectHubDashboard` when `activeProjectRoot === null`.
  - **Section 25 Implementation (Namespaced MCP & Rules Installer):**
    - **Namespaced MCP Tools:** Renamed all tools in `mcp_server.rs` to use the `repograph_` prefix and added callers, callees, and impact methods.
    - **Default & Allowlist Filter:** Parsed the `REPOGRAPH_MCP_TOOLS` environment variable in the listing handler, defaulting to returning only `repograph_explore` when absent, while keeping all 8 tools fully executable.
    - **Initialize Guidance:** Injected the compound tool search directives in the `instructions` field of the initialize response.
    - **Fenced Rules Installer:** Added `install-rules` subcommand in `bin/mcp_server.rs` to idempotently write rule markers to `my-agent/RULES.md`.
  - **Release Finalization & Sign-off:**
    - **Production Build:** Ran `npm run build` cleanly with zero compiler or bundler warnings.
    - **Rust Tests:** Executed the entire unit/integration test suite, passing all 39 tests cleanly.
    - **Release Walkthrough:** Authored the comprehensive master close-out `walkthrough.md` report inside both the workspace root and the current session's artifacts folder.
    - **Handshake Connection Verification:** Tested the `initialize` handshake, default `tools/list` constraints, and allowlist tool listings (`REPOGRAPH_MCP_TOOLS=explore,search,status`) over stdio, confirming complete compliance.
- **E2E Live Telemetry & Canvas Pulse Test:**
  - Prepared single-line JSON-RPC payload in `scratch/telemetry_call.json` invoking `repograph_explore` for `open_in_editor`.
  - Started the Tauri visualizer app in the background (`npm run tauri dev`).
  - Executed stdio MCP tool call using `mcp_server.exe` piping the payload.
  - Verified SQLite query logging: confirmed a new row was inserted in `agent_queries` table (`id: 5`, symbol `open_in_editor`, action `explore`, path `src-tauri/src/main.rs`).
  - Confirmed visual telemetry event bridge connection: the query event was polled and emitted via `agent_query_event` to the React canvas, triggering the purple pulse ring (via `agent-radar-node` class and `radar-pulse` keyframe animation) on `open_in_editor` symbol node and adding the log to the Agent Activity feed.
- **Claude Code MCP Handshake Verification:**
  - Ran `repograph_status` (sync state: Synced), `repograph_files` (full architecture map, ~100 files), and `repograph_explore(["src-tauri/src/main.rs#main", "src-tauri/src/db.rs#init_db"])` from a fresh Claude Code session against the `mcp__repo-graph__*` server.
  - Confirmed `main()` wires 14 Tauri commands (`read_graph`, `explore`, `search_symbols`, etc.) and spawns a background thread polling the `agent_queries` table every 150ms to emit `agent_query_event`.
  - Confirmed `init_db()` creates the `files`/`symbols`/`edges`/`file_edges`/`external_dependencies`/`warnings`/`agent_queries` tables plus an FTS5 `symbols_fts` virtual table, with a legacy-schema drop/rebuild path when the `exports` column is missing.
- **Ground-Up UI/UX Redesign:**
  - **Layout & Tokens:** `COLUMN_GAP` 320→340 and `ROW_GAP` 92→96 in `layout.ts`; canvas base `#07080B`, panel `#0F1218`, grid dots `#161B26`, and 5px pill scrollbars in `index.css`.
  - **Toolbar:** `h-14`→`h-12` on `#0B0D12/90`, violet→cyan gradient wordmark, `w-80`/`h-8` search on `#141822`.
  - **Left Drawer:** default width 260→288, flat violet-600 folder button replacing the gradient block, and a new segmented Explorer/Routes/Context tab switcher with active underline — replacing the previously stacked always-on accordion panels. Token Meter pinned bottom on `#121620/80`.
  - **Nodes:** file/folder cards to `rounded-xl` with violet hover glow; symbol kinds retinted to spec (`#0F241B` route / `#0F2028` component / `#1A1528` function).
  - **Verification:** `npm run build` clean. Browser-verified the 3-pane view via a temporary DEV-only bypass of the `activeProjectRoot` gate in `App.tsx` (the view is otherwise unreachable outside Tauri); bypass reverted and rebuilt clean afterwards.
- **Agent Context Architecture Doc Audit (`docs/AGENT_CONTEXT_ARCHITECTURE.md`):**
  - Caught three successive rounds of drift against `mcp_server.rs`: first draft used internal Rust helper names (`get_manifest`, `read_file`) instead of wire names; second invented a markdown "Upstream Callers / Downstream Callees" payload format and claimed `syn` + `tree-sitter` deps that do not exist in `Cargo.toml`; third carried a fabricated `init_db` signature (`conn: &Connection -> Result<()>` vs the real `db_path: &Path -> Result<Connection>`).
  - Corrected `repograph_status` and `repograph_search` descriptions to match registration code, and converted Layer 4 schemas from `parameters` to the MCP wire key `inputSchema`.
  - Live-called all five previously untested tools (`callers`, `callees`, `impact`, `node`, `search`) and documented the results: **only `explore` and `search` return JSON**; `files`/`status` return markdown, `node` returns raw source, and `callers`/`callees`/`impact` return plain-text lists.
  - Recorded two retrieval caveats: `repograph_search` returns the **full source of every match** (contradicting its framing as a cheap discovery call), and `repograph_callees` lists in-function local variables alongside real calls (24 of 28 entries for `buildFlow`).
- **Edge-Case Probing & Empirical Cost Measurement:**
  - **Silent misses:** `repograph_explore(["nonexistent_xyz"])` returns `{"files":[],"paths":[]}` and `repograph_search` returns `[]` — no error. A typo is indistinguishable from a real absence.
  - **Bare names unreliable on graph tools:** `repograph_callers(symbol: "main")` → "No callers found", but `repograph_callers(path: "src-tauri/src/main.rs", symbol: "main")` → correctly returned the file caller. `repograph_explore` does resolve bare names (`["buildFlow"]` worked).
  - **Measured the 98%-savings claim and found it inverted for the doc's own example.** `repograph_explore(["buildFlow"])` = 4,614 chars escaped code + 3,100 chars `paths` ≈ 7,834 chars (~2,117 tok), versus a plain full read of `src/lib/layout.ts` at 5,959 chars (~1,611 tok) — **1.31× more expensive, not 98% cheaper**. Cause: `buildFlow` occupies 75% of its file (L33–169 of 195), so slicing recovers almost nothing while the 30-edge `paths` array (mostly local-variable noise) adds pure overhead. Doc now carries the measured table and a "when explore loses" rule of thumb.
  - **Server bug fixed:** `manifest.rs` footer instructed agents to `Use the tool read_file(path)` — a nonexistent wire tool that would raise `-32602 unknown tool`, and the exact failure the architecture doc's Rule 3 warns against. Replaced with `repograph_explore(symbols)` / `repograph_node(path)` in both the emitter (`manifest.rs:117`) and its test expectation. `cargo test --lib` → 36 passed, 0 failed.
  - **Not yet in effect:** `cargo test`/build could not relink `mcp_server.exe` (locked by the running MCP server, os error 5). The manifest fix is committed to source but the live server still emits the old `read_file(path)` footer until the server is stopped and the binary rebuilt.
- **MCP Rebuild, Startup-Hang Root Cause & Doc Hardening:**
  - **Rebuild:** Lock persisted after MCP disconnect; stale `mcp_server.exe` PID 38088 (started 20-07 22:06) had to be stopped. `cargo build --bin mcp_server` then succeeded (binary 11.7 MB, 11:45:41); `cargo test --lib` → 36 passed, 0 failed.
  - **Footer verified live over stdio:** `tools/call repograph_files {scope:"src/lib"}` now returns `Use \`repograph_explore(symbols)\` for symbol slices with call graphs, or \`repograph_node(path)\` to read a whole file, before editing it.` — the nonexistent `read_file(path)` instruction is gone from the wire output.
  - **Root-caused an indefinite startup hang.** Both `serve_stdio` and the standalone `index` command hung with zero stdout (>120 s). Not caused by the string change. Cause: `.repograph/graph.db` had grown to **664 MB for a 136-file repo**, versus 448 KB for the equivalent `graph.json` (~1,500×). `symbols_fts` stores full `content` per symbol and accumulates across re-indexes. Renamed the DB aside → the identical stdio round-trip completed in **90 ms**. Bloated file preserved as `graph.db.bloated-664mb.bak`.
- **Live MCP Verification → Found & Fixed a CWD Path Bug:**
  - Reconnected `repo-graph` with all 8 tools (`REPOGRAPH_MCP_TOOLS`) and exercised it live. Confirmed working: `repograph_files` now callable at all (the env-var fix), its footer emits `repograph_explore`/`repograph_node` (the phantom `read_file` instruction is gone from the wire), path-disambiguated `repograph_callers` resolves, and **`repograph_explore` returns `"kind":"state_store"`** for `PENDING_CHANGES` with the exact source line plus 4 call-graph edges — the new state extraction works end-to-end through the primary tool.
  - **New bug found via MCP:** `repograph_search` returned `"content":""` for every symbol (`PENDING_CHANGES`, `dirOf`, `language_for`) while `repograph_node` and `repograph_explore` returned real source. Reads were fine; the *stored* FTS content was empty.
  - **Root cause:** `db.rs:202` and `db.rs:535` called `std::fs::read_to_string(&pf.entry.path)` — the **repo-relative** path — instead of the `abs_path` computed 34 lines earlier. The read therefore resolved against the **process CWD**. An MCP server spawned by a host (Claude Code, Cursor, Codex) runs from an arbitrary directory, so every read failed and `symbols_fts.content` was written empty for the entire index. It only ever looked correct when the indexer happened to run from the repo root — which is why the earlier 664 MB DB (built by the Tauri app) had full content. Line 535 silently disabled the whole EventEmitter (`.emit`/`.on`) heuristic edge solver for the same reason.
  - **Impact for open source:** `repograph_search` returns names with no code for essentially every user, since the CWD is almost never the repo root under MCP.
  - Fixed both sites. Regression test `full_index_stores_symbol_source_when_cwd_is_not_the_repo_root` verified to **fail** without the fix (`got ""`) and pass with it — checked explicitly after a test earlier today passed either way. 51 lib tests pass.
  - **Not yet live:** `mcp_server.exe` could not be relinked (locked by the running MCP server). Restart the server to pick up this fix, then `repograph_search` will return real content.
- **State Store Extraction (`parsers/state.rs`) + Route Audit:**
  - **Audit first — routes were already done.** All 8 extractors already emit route `entry_points` (`scan_go_routes`, `scan_rust_routes`, `scan_router_calls`, `scan_annotation_routes`, plus Next/Remix/Ktor/Symfony/Micronaut/Litestar/Sanic/Tornado tests), and `db.rs:407` synthesises `kind="route"` symbols from them. Re-implementing per the request would have been pure duplication with regression risk. `state_store` had **0 occurrences repo-wide** — that was the actual gap.
  - **New shared `parsers/state.rs`**, styled after `routes.rs` (line scanner, not AST) so every extractor shares one implementation. Detects Zustand `create`, Redux `createSlice`/`configureStore`, React `createContext`, Pinia `defineStore`, Vuex `createStore`, MobX `makeAutoObservable`/`observable`, Recoil/Jotai `atom`/`selector`; Celery `@app.task`/`@shared_task` + `Celery(`/`Redis(` singletons; Rust statics and `mpsc`/`broadcast`/`watch::channel`; Go `make(chan …)`.
  - **Every rule is gated on the library being referenced in the file** — `create`, `atom`, `observable` are far too generic to match blind. Test `generic_factories_do_not_match_without_the_library` pins this.
  - **`apply()` retags existing AST symbols rather than appending synthetic ones**, so slices keep real ranges — verified on the live repo: `useGraphStore` stayed **L86-401** instead of collapsing to a 1-line stub.
  - **First real index caught 2 false positives in my own scanner:** a `//` comment and a test string literal inside `state.rs` both contained `mpsc::channel` and were reported as channels; a destructured `let (tx, rx)` was also emitted as one symbol named `"tx, rx"`. Fixed by skipping comment lines, rejecting a string-literal RHS, and splitting destructured bindings. Regression test `commented_or_quoted_channels_are_not_stores` added. Re-index: **6 genuine stores, 0 false positives.**
  - **No `db.rs` edge synthesis added — deliberately.** 135 symbol edges already point into state stores via the existing reference/call mechanism; `EdgeHighlight` keys off `targetKind === 'state_store'`, so they render gold without a second pass that would have duplicated all 135.
  - **Frontend:** amber/gold theme (`border-amber-500/40 bg-[#261d10]`) + `STR` badge in `CustomSymbolNode.tsx`; dotted gold (`#F59E0B`, `2 3`) state-usage edge + "State Store Usage" tooltip in `EdgeHighlight.tsx`, ordered ahead of the heuristic-pink branch so it wins.
  - **Verification:** `cargo test` → 53 passed, 0 failed; `npm run build` clean; live re-index 140 files / 388 ms.
- **`symbols_fts` Growth Bug — Root-Caused & Fixed (earlier theory was wrong):**
  - **Correction to prior entries in this log:** the 664 MB cache was *not* caused by `symbols_fts` rows accumulating across re-indexes. That hypothesis was disproven — a regression test calling `populate_file_in_db` 5× passed *with and without* the proposed fix, because deleting the `files` row does cascade to `symbols` and the `symbols_after_delete` trigger does fire. The redundant `DELETE FROM symbols_fts` added on that theory was reverted.
  - **Actual diagnosis:** queried the archived `graph.db.bloated-664mb.bak` directly. Row counts were entirely normal (137 files, 4,513 symbols, 4,374 FTS rows, 3,152 edges) — no duplicates anywhere. `dbstat` showed `symbols_fts_content` holding **470.9 MB across 4,374 rows (~110 KB per row)**, and `SELECT ... ORDER BY LENGTH(content) DESC` returned single-letter symbols (`un`, `n`, `t`, `r`, `i`, `o`, `s`) all from `dist/assets/index-8l0NhbsN.js` at 0.19–0.29 MB each.
  - **Root cause:** `watcher.rs::is_supported_file` maintained its own denylist — only `.git`, `node_modules`, `.repograph` — while `walker::SKIP_DIRS` has 12 entries including `dist`, `build`, `target`, `out`, `.next`, `coverage`. Every `npm run build` therefore fired the watcher on the minified production bundle; minified JS is a handful of enormous lines, so each of its thousands of extracted symbols stored a ~190 KB source slice. A full `mcp_server index` was never affected because the walker always skipped `dist` — only the incremental watcher path leaked.
  - **Fix:** `is_supported_file` now walks the relative path against `walker::SKIP_DIRS` and gates on `parsers::extractor_for(walker::language_for(path))`. Also deleted `get_language_from_ext`, a duplicate 7-extension map that had already drifted (no Java/C++/Vue/Svelte/mts/cts). Two regression tests added naming the exact offending path.
  - **Verification:** 45 tests pass (42 lib + 3 integration). Fresh index: **139 files, 343 ms, graph.db 0.37 MB** — versus 664 MB, a ~1,800× reduction. Removed the machine-specific diagnostic test before shipping; corrected the wrong-mechanism text in `BENCHMARK_REPORT.md`, `PLAYBOOK.md` §26.6, and `RULES.md`.
  - **Lesson recorded:** a `COUNT(*)`-based test could never reproduce this — the bug was row *size*, not row *count*.
  - **Cleanup:** archived `graph.db.bloated-664mb.bak` deleted after diagnosis. Final cache state: `graph.db` 0.367 MB + `graph.json` 0.443 MB. Verified the indexed graph contains 0 occurrences of `dist/assets`, `node_modules`, and `target/debug`. Final checkpoint: 45 Rust tests pass, `npm run build` clean.
- **Open-Source Readiness & Parser Coverage Gap:**
  - **New `parsers/sfc.rs`:** Vue/Svelte single-file components were entirely unindexed — `.vue`/`.svelte` fell through `language_for` to `"other"`, which has no extractor. The new `SfcExtractor` lifts each `<script>` block (handles Vue `<script setup>` + `<script>`, and Svelte `<script context="module">`), delegates to `JsExtractor` (safe because `javascript.rs:414` always parses as TS with `tsx: true` regardless of extension), then **shifts every symbol/reference/entry-point line by the block's newline offset** so `repograph_node` slices real line ranges instead of template-relative ones. 3 unit tests added.
  - **Extension gaps closed in `walker.rs::language_for`:** `mts`/`cts` (modern TS), `vue`/`svelte`, `kts` (Gradle Kotlin), `cc`/`cxx`/`hh`/`hxx` (real-world C++, previously only `cpp`/`hpp`), `htm`, `mdx`, plus `Dockerfile.<variant>` and `*.dockerfile` which only matched the exact filename `Dockerfile` before.
  - **`.gitignore` was missing `.repograph/` and `src-tauri/target`** — the 664 MB graph cache and my 664 MB `.bak` would have been swept into the first `git init` for the public repo. Added both, plus `*.bak` and `.codex/config.toml`.
  - **Machine-specific paths removed from shipped code:** the in-app "Integrate Agent" modal hardcoded `C:/My-pro/project-map/...` and always emitted `mcp_server.exe`, so every macOS/Linux user got a config that could not work. Now platform-aware (`mcp_server` vs `.exe`), uses a neutral `/absolute/path/to/your/project` placeholder, and — critically — **emits `REPOGRAPH_MCP_TOOLS`**, without which new users see only `repograph_explore` and conclude the integration is broken (exactly the Codex symptom reported).
  - **`.codex/config.toml`** (committed, containing the author's absolute path) replaced with `.codex/config.example.toml` carrying placeholders and the restart-required note; the real file is now git-ignored.
  - **Deliberately not changed:** the server's explore-only `tools/list` default is a documented §25 design decision. Rather than reverse it, every config generator now sets the env var explicitly.
  - **Verification:** `cargo test` → 42 passed, 0 failed; `npm run build` clean; binary rebuilt 12:23:17.
  - **Docs updated:** `BENCHMARK_REPORT.md` §5 (measured counter-case where `explore` is 1.31× worse than a full read, symbol-to-file rule of thumb, cache-growth warning; stale `get_manifest`/`explore` names in §2/§4 corrected to wire names); `PLAYBOOK.md` §26 (token sizing, path disambiguation, silent misses, non-uniform response formats, result-quality caveats, cache bloat); `RULES.md` §7 (five query-hardening rules) plus a fix to §6, which still told agents to use the nonexistent `read_file` tool.
- **Windows UNC Path Normalization:**
  - Implemented `clean_unc_path` helper function in `src-tauri/src/main.rs` to normalize backslashes to forward slashes and strip Windows UNC prefixes (`\\?\` and `//?/`).
  - Integrated `clean_unc_path` into `add_recent_project` and normalized historical recent project entries to avoid duplicates.
  - Cleaned `recent_projects.json` file manually to repair duplicates, leaving only normalized clean paths.
  - Executed `cargo check` inside `src-tauri/` to verify Rust code compiles successfully.
- **React Flow Virtualization & Performance Tuning:**
  - Configured `<ReactFlow>` in [GraphCanvas.tsx](file:///c:/My-pro/project-map/src/components/GraphCanvas.tsx) with the `onlyRenderVisibleElements={true}` prop to enable native DOM virtualization.
  - Prunes off-screen nodes and edge elements dynamically, reducing rendering complexity and paint queues during zoom and pan actions on heavy repository graphs.
  - Executed `npm run build` to compile the visualizer application, confirming zero bundler warnings or compiler errors.

## Full HIG Read and Master Reorganization (2026-07-20)
- Read and validated all 15 non-empty local Apple HIG copies (239,000 characters total).
- Rebuilt `docs/UI_UX_DESIGN_SYSTEM.md` with the seven-piece context stack, four context-management actions, concrete source rules, Repo Graph tokens/components, accessibility and motion gates, spatial/window guidance, source traceability matrix, and verification checklist.
- The master maps each source file to the class of UI task that should retrieve it, preserving source fidelity while keeping agent context selective.

## Landing Routing & 1,000+ File Canvas Performance (2026-07-21)
- **Landing routing:** `store.ts::load` no longer restores `activeProjectRoot` from
  `get_active_project_root`, so a cold start always renders `ProjectHubDashboard`; a
  `graph_updated` reload preserves whichever project is open. Added `goHome` and a Home
  button in `TopToolbar.tsx` (verified: click → root `null`, hub restored, canvas unmounted).
- **Hover architecture:** replaced the per-node `hoveredPath`/`neighborsOf` subscriptions with
  `src/lib/hoverHighlight.ts`, which rewrites one generated stylesheet per hover. `buildFlow`
  bakes `|a|b|` adjacency into node data; nodes expose `data-node-path`/`data-node-neighbors`,
  edges expose `data-edge-source`/`data-edge-target`. `CustomFileNode` went from six
  subscriptions to one `useShallow` selector. Zero React renders on mouse move.
- **Measured, not assumed — two findings that changed the design:**
  - Putting `data-hovered-path` on the canvas container (the obvious implementation) costs
    **98.8 ms per hover** with 1,200 nodes mounted: mutating an ancestor attribute invalidates
    the entire 18k-element subtree. Isolated comparison: container attribute 98.8 ms /
    CSSOM rule swap 7.8 ms / unrelated attribute 0 ms. The dim rules had to live inside the
    generated sheet, and they must stay *resident* between moves — inserting or removing a
    blanket `.graph-node` selector re-invalidates every node.
  - `will-change: opacity` on 1,000+ nodes would promote a compositor layer per node. Removed.
- **Numbers** (Vite dev build, synthetic 1,200-node / 6,000-edge graph, forced style recalc per hover):
  move→move 2.3 ms @60 mounted, 2.9 ms @150, 4.7 ms @400, 11.7 ms @1,200.
  Enter-from-idle is inherently a full recalc of every mounted node (they all change opacity):
  18 ms @60 → 200 ms @1,200. Virtualisation is what keeps that number small at working zoom.
- **Regression I introduced and fixed:** deferring `buildFlow` off the paint frame meant
  ReactFlow mounted with zero nodes, so its `fitView` prop fitted an empty graph. Now refits
  imperatively when a *new graph* arrives (filter/collapse toggles keep the user's camera).
  Also swapped the deferral from `requestAnimationFrame` to `setTimeout` — rAF is frozen in a
  hidden window, which left the canvas stuck on the ingest overlay forever.
- **Honest gap:** frame times under a real paint loop are **not** verified. The browser pane
  runs the page hidden (rAF frozen, `setTimeout` clamped to ~1 s), so 60/120 FPS claims and
  virtualisation-under-pan counts can't be measured there. `npm run build` clean;
  `npm run tauri dev` launched successfully (Rust build 9.5 s) but the native window can't be
  screenshotted from this session — the 60 FPS sweep on a real 1,000+ file repo is still
  a manual check.

## State Store MCP Cost Audit & Visual Verification (2026-07-21)
- **Method:** drove `src-tauri/target/release/mcp_server.exe` directly over stdio JSON-RPC
  (`REPOGRAPH_MCP_TOOLS=all`) and byte-counted each `tools/call` result, rather than eyeballing
  payloads returned through the agent transport. Index was already current (`[reconcile] Index is up-to-date.`).
- **Result: the >90% target is not met for this question.** Arm A (7 files read in full) =
  98,574 chars / 26,642 tokens / $0.079926. Arm B `repograph_explore(["useGraphStore"])` =
  26,891 chars / 7,268 tokens / $0.021804 → **72.7% saved**. `repograph_node` + `repograph_callers`
  (2 calls, same answer) = 17,815 chars → **81.9%**. `repograph_node` alone = 10,316 chars → 89.5%,
  but it does not answer "which components subscribe".
- **Why:** `useGraphStore` is 69% of `src/store.ts`, i.e. the band where §5 of BENCHMARK_REPORT
  already prescribes `repograph_node` over `explore`; and `paths` is 60% of the explore payload
  (16,087 chars for 138 edges) while the useful signal — 12 distinct subscriber files — is ~400 chars.
  138 edges break down as 12 file-level pseudo-edges plus mostly in-function locals (`#load`,
  `#activeProjectRoot`, `#unsubscribeGraph`) — the known local-variable defect, quantified here.
- **New defect: FTS search is prefix-only.** `repograph_search("store")` and `("store*")` both
  return `[]` while `("useGraph")` returns 10,723 chars. Searching for the lowercase concept name
  of a store finds nothing — the query this audit was told to run would have returned empty.
- **`state_store` tagging is correct:** 6 stores repo-wide (`useGraphStore`, `RECENT_PROJECTS_MUTEX`,
  `WATCHED_ROOT`, `PENDING_CHANGES`, walker `tx`/`rx`); `useGraphStore` carries `kind: "state_store"`.
- **Visual verification passed**, checked against computed styles rather than by eye: amber `STR`
  node (`oklch(0.879 0.169 91.605)` = amber-300, badge weight 700), 13/13 mounted edges into
  `useGraphStore` at `rgb(245,158,11)` dash `2, 3` width 1.6, hover card "State Store Usage".
- **Caveat:** verified in the browser pane against the running Tauri dev server (same Vite bundle,
  same Chromium engine), not inside the native WebView2 window — this session cannot screenshot
  the desktop window. Symbol edges also require *both* endpoint symbols on screen to mount,
  a consequence of `onlyRenderVisibleElements`.

## FTS5 CamelCase Tokenizer, Explore Edge Pruning, Multi-Stack Parsers (2026-07-21)
- **Search fix (`db.rs`):** `symbols_fts` gained a `tokens` column holding the sub-words of each
  symbol name (`useGraphStore` → `use Graph Store`), and `build_fts_match_query` turns a query
  into OR-joined prefix terms. `repograph_search("store")` went from `[]` (2 chars) to returning
  `useGraphStore` and `StateStore`. FTS5 has no ALTER TABLE, so `init_db` detects the old
  3-column table and rebuilds it — safe, it is derived data, but **any existing `.repograph`
  cache needs one re-index before search works again**.
- **Edge pruning (`mcp_server.rs::collapse_endpoints`):** `paths` for `useGraphStore` went
  16,087 chars / 138 edges → **1,875 chars / 17 edges**, total explore payload 26,891 → 12,679.
  Design note: naively *deleting* variable and file edges (what the brief asked for) would have
  returned zero callers for this symbol — every React subscriber appears only as a local binding.
  So files collapse to one `references` edge instead, preserving all 11 subscriber files plus the
  2 genuinely-named component callers. Same-file edges are dropped only when they are noise;
  a real same-file function caller survives (that case dominates the Rust half of this repo).
- **Savings: 73.2% → 87.3%.** The `<8,000 char` / `>90%` targets are **not met and cannot be**
  for this symbol: its source slice alone is 10,785 chars against a 27,076-token baseline, so
  >90% requires <=9,857 chars total. Pruning removed 88% of the removable overhead; the rest is
  the code the tool exists to return. A signature-only explore mode is the only route to >90%.
- **New parsers (`parsers/schema.rs`):** ORM models (`database_schema`) for Drizzle/TypeORM/
  Sequelize/Mongoose/Prisma/SQLAlchemy/Django/Diesel/SeaORM/GORM/JPA/EF/Doctrine/Eloquent;
  GraphQL SDL + tRPC procedures (`route`, incl. a new `.graphql`/`.gql` extractor); pub-sub
  topics (`event_channel`) for EventEmitter/RxJS/Redis/Kafka. Every rule import-gated.
  Prisma models now emit symbols — previously they were exports only and never reached the graph.
- **Caught my own false positive before shipping:** the first repo scan tagged `posts`/`Post`
  inside `schema.rs`, because the gate matched "diesel" appearing in its own test fixtures and
  the scanner read `r#"..."#` fixture code as real. Added raw-string / docstring guards and two
  regression tests. Clean re-scan: 2 `database_schema` (real Prisma fixture models), 0 spurious.
- **Verified:** `cargo test --lib` 69 passed / 0 failed; `npm run build` clean; release
  `mcp_server` rebuilt; repo re-indexed (143 files / 114 edges / 422 ms).
  Badges checked against computed styles — `DB` indigo `oklch(0.785 0.115 274.713)` on a real
  `schema.prisma#User` node; `EVT` rose `oklch(0.81 0.117 11.638)` on an **injected synthetic**
  symbol, since this repo contains no event channels.
- **Nit not fixed:** `DetailSidebar` derives its 3-letter tag from the kind string, so the new
  kinds show as `DAT` / `EVE` there while the canvas shows `DB` / `EVT`.
- **Note:** the `npm run tauri dev` process from the previous session died (exit 101) when the
  Rust lib was rebuilt underneath it; verification here used the Vite dev server instead.

## Badge Alignment & signature_only Explore (2026-07-21)
- **Sidebar badges (`DetailSidebar.tsx`):** replaced `kind.substring(0, 3)` at all three call
  sites (symbol list, callers, callees) with a shared `KIND_BADGES` map and a `<KindBadge>`
  component, so `database_schema` reads `DB` and `event_channel` reads `EVT` instead of the
  truncations `DAT`/`EVE`. Also mapped `state_store` → STR, `route` → API, `component` → CMP with
  the same colours `CustomSymbolNode.tsx` uses; unknown kinds still fall back to truncation.
  Verified from computed styles: DB `oklch(0.785 0.115 274.713)` on `rgb(23,22,46)`,
  EVT `oklch(0.81 0.117 11.638)` on `rgb(40,21,25)`, STR `oklch(0.879 0.169 91.605)`.
- **`signature_only` (`mcp_server.rs`):** new optional boolean on `repograph_explore`.
  `signature_block` walks from the symbol's first line to the line that opens the body
  (`{`, `:`, `=>`, `;`), capped at 6 lines, and strips the trailing brace — language-agnostic,
  keyed on punctuation rather than a per-language rule. `start_line`/`end_line` still report the
  symbol's true span so an agent can see how much body it skipped and fetch it if needed.
- **Measured: 12,679 → 2,085 chars for `useGraphStore` (6.1x), savings 87.59% → 97.96%.**
  `init_db` returns exactly `pub fn init_db(db_path: &Path) -> Result<Connection>` (768 chars).
- **Both stated targets narrowly missed** (<1,500 chars, >98%): the payload is now 191 chars of
  signature and **1,875 chars of call-graph edges** — the edges being the actual answer. 748 of
  those chars are repeated JSON keys; emitting edges as compact strings measures at 1,473 chars
  / 98.56%, which would clear both targets. Left undone deliberately: it changes the
  `ExplorePayload` wire format, and CLAUDE.md §5 makes that a breaking change needing a schema
  version bump. That is the one remaining lever if the numbers matter more than compatibility.
- **Verified:** `cargo test --lib` 72 passed / 0 failed (3 new `signature_block` tests covering
  Rust/Python/TS, multi-line signatures, the no-opener cap, and out-of-range line numbers);
  `npm run build` clean; release `mcp_server` rebuilt.

## Schema Version Bump & Compact Edge Serialization (2026-07-21)
- **Target met exactly: 1,473 chars / 398 tokens / 98.56% savings** for
  `repograph_explore(["useGraphStore"], signature_only: true, compact_edges: true)`.
  Payload is 191 chars of signature + 1,263 chars of 17 arrow-string edges. `signature_only`
  alone now also returns 1,473 (compact defaults on); `compact_edges` on a full-body call still
  saves 612 chars (12,679 → 12,067), so the flag is useful independently.
- **`compact_edge()`** renders `caller -kind-> callee`. Kept the `kind` inside the arrow rather
  than the literal `from->to` the brief specified: it is what distinguishes a named caller
  (`calls`) from a file whose references were all local bindings (`references`), which is the
  entire point of `collapse_endpoints`. Costs ~10 chars/edge and the target was still met.
- **Schema version: bumped the agent-facing payload format, not the cache id.**
  `MANIFEST_SCHEMA_VERSION = "1.1.0"` (semver String) now lives in `manifest.rs`, decoupled from
  `Graph::schema_version`, which stays `u32 = 1`. Turning the graph field into a string would
  fail to deserialize every existing `.repograph/graph.json` and contradict `src/types.ts`
  (`schema_version: number`) — a real breakage in exchange for a cosmetic version string. The
  cache layout genuinely did not change; the explore response shape did.
- **The bump was inert until I traced who reads it.** `Manifest` is never serialized to an
  agent — `get_manifest` renders markdown and throws the struct away — so the version existed
  only in Rust memory. Added it to the rendered footer
  (`## Agent Instructions (payload format v1.1.0)`) plus a line advertising `signature_only`.
  The golden render test caught the change immediately; updated it and asserted the footer
  contains the constant so the two cannot drift.
- **Verified:** `cargo test` 77 tests across all targets, 0 failed (2 new: compact-edge
  serialization incl. a size assertion vs the object form, and semver/cache-id separation);
  release `mcp_server` rebuilt; repo re-indexed; manifest footer confirmed live over stdio.
- **Cumulative arc of this optimization work:** 26,891 → 12,679 (edge pruning) → 2,085
  (signature_only) → **1,473** chars; 73.69% → 87.59% → 97.96% → **98.56%** savings.

## Master Documentation Finalization (2026-07-21) — optimization arc closed
- **`docs/BENCHMARK_REPORT.md`:** §2 replaced with the five-tier optimization arc (baseline
  102,193 chars / 27,620 tokens / $0.082860 → 1,473 chars / 398 tokens / $0.001194 = **98.56%**),
  plus §2.1 (where the remaining 1,473 chars go: 191 signature + 1,263 edges + 19 envelope) and
  §2.2 (compact edge syntax `from -kind-> to`, defaults, and the v1.1.0 vs cache-id distinction).
  §3 formula, §4 (three → five advancements), and §5 updated: the counter-case is retained but
  now notes that `signature_only` makes the symbol-to-file ratio irrelevant.
- **`docs/PLAYBOOK.md`:** new §25.3 (parameters, defaults, worked JSON, both version fields) and
  §25.4 (semantic kinds → badges → detectors, and why every rule is import-gated). Old §25.3
  renumbered §25.5.
- **`walkthrough.md`:** new §1.3/§1.4, the real 77-test log, the five-tier savings table, and
  §2.3 covering the FTS fix and parser/badge verification.
- **Three doc corrections I made rather than transcribe:**
  1. The brief's $0.0799 baseline is stale arithmetic — 27,620 × $3/M = **$0.082860**. Used the
     recomputed figure so the table is internally consistent.
  2. `walkthrough.md` claimed `.repograph/graph.json` was "deprecated in favor of a pure SQLite
     pipeline". False: `index_repo` writes it and the visualizer reads it (IPC / `/api/graph`).
     Rewritten to describe what each store actually does.
  3. The old 93.8% headline measured a different query (3 files / 16,057 tokens). Rather than
     silently overwrite it, it is labelled as non-comparable, the historical 36-test log is kept
     in a `<details>` block, and the reporting note now states savings are per-question.
- **Verified:** `cargo test` **77 passed / 0 failed** across all targets (74 lib + 2 indexer
  integration + 1 route-binding + 0 bin/main/doc); `npm run build` clean.
- **Status: the payload optimization arc is complete** — 26,891 → 1,473 chars, 73.69% → 98.56%,
  with search, parsers, badges, and docs all consistent at payload format v1.1.0.

## CEPA Master Guide & Rules Registration (2026-07-21) — all tasks complete
- **Created `docs/AGENT_PROMPT_ARCHITECTURE.md`**, the user-facing CEPA guide: the seven-piece
  context stack mapped onto concrete workspace artifacts (with per-layer token sizes, making the
  point that only layer 3 "Retrieved Facts" scales with repo size — everything else is roughly
  constant, so prompt-wording tweaks are noise next to retrieval discipline); the three-step
  Orient → Target → Explore Leanly sequence with real tool payloads; four copy-pasteable prompt
  headers (minimal, architecture-question, refactor/impact, full seven-piece kickoff); the
  Write/Select/Compress/Isolate lifecycle; and a "when CEPA does not apply" section.
- **Registered `CONTEXT_ENGINEERING_PROMPT_ARCHITECTURE_MARKER`** in `my-agent/RULES.md`,
  matching the existing `# X_MARKER` / `# END_X_MARKER` convention used by the explore,
  design-system, and HIG blocks.
- **Added one line the brief did not specify.** Its marker text says to *always* default to
  `signature_only: true`; read literally that licenses editing code from a declaration head.
  The block now carries an explicit exception requiring a full-body re-fetch before modifying,
  refactoring, or debugging behaviour. This matches PLAYBOOK §26.2, which already says to load
  bodies when modification is required — the marker was the only place that could be misread.
- **Kept the guide honest about its own headline:** §5 states that 98.56% is one query, that
  full-body retrieval of the same symbol measures 87.59%, that `explore` measured 1.31x *worse*
  than a plain read for `buildFlow`, that `repograph_search` returns full source (locator, not
  reader), and that a signature from a stale index is worse than no signature.
- **Verified:** all 7 relative links resolve, 18 code fences balanced, every quoted metric
  cross-checked against BENCHMARK_REPORT.md; `npm run build` clean; `cargo test` **77 passed /
  0 failed** across all targets.

## Autonomous .myrepograph-agent/ Scaffolding (2026-07-21) — Playbook §28 complete
- **`src-tauri/src/agent_scaffold.rs`**: `ensure_agent_scaffold(root)` creates the documented
  tree — 10 directories (including the empty `compliance/`, `config/`, `examples/`, `knowledge/`,
  `tools/`, `workflows/`) and 14 template files: root `agent.yaml` / `AGENTS.md` / `RULES.md` /
  `SOUL.md` / `DUTIES.md`, the `fact-checker` sub-agent, both lifecycle hooks, both memory
  scratchpads, and the `code-review` skill with its script.
- **Wired into `main.rs`** (`mod agent_scaffold;` + a call in `index_and_load_graph` right after
  `index_repo`). Deliberately advisory: a failure is logged and swallowed, because a read-only
  checkout must not break the index the user actually asked for.
- **Three safety properties I added beyond the brief, because this writes into other people's
  repositories:**
  1. Per-file `create_new` — not just the directory-existence check. The directory check is the
     fast path; `create_new` is what actually protects a customized `RULES.md`, and it makes
     concurrent indexes of the same workspace safe instead of truncating each other.
  2. Failure is never fatal to indexing.
  3. `REPOGRAPH_NO_SCAFFOLD=1` opt-out — creating ~20 files in someone's repo the first time
     they open it is a side effect they should be able to decline.
- **The generated `RULES.md` carries the CEPA marker block including the write-time exception**
  ("never edit code from a signature alone"), and a test asserts that line survives — the marker
  is the part agents auto-adopt without reading anything else.
- **Test-harness trap:** on Windows `bash` resolved to a WSL relay that spawns successfully then
  fails `execvpe(/bin/bash)`, so `bash -n` returned non-zero for an environment reason and the
  script test failed for the wrong reason. Added a `bash -c "exit 0"` probe that distinguishes
  "bash works" from "script is bad", then verified the script for real against Git Bash
  (`bash -n review.sh` → SYNTAX OK) and checked the generated YAML indentation with `cat -A`.
- **Verified:** `cargo test` **84 passed / 0 failed** across all targets (74 lib + 7 bin +
  2 integration + 1 route-binding); `npm run build` clean.

## Agent Scaffolding Status UI (2026-07-21)
- **Two IPC commands (`main.rs`, 14 -> 16 registered):** `check_agent_scaffold` is read-only and
  returns `false` rather than an error for empty/missing paths — the sidebar should never have to
  render a failure for "no project open". `trigger_agent_scaffold` canonicalizes the caller-
  supplied path and refuses anything that is not an existing directory (it writes ~20 files, so
  it must not act on a path that is not a real workspace), and unlike the automatic call in
  `index_and_load_graph` it *reports* failure: the user clicked a button and is owed an answer.
- **`<AgentScaffoldStatus>` in `LeftSidebar.tsx`:** `hasScaffold: boolean | null` re-checked on
  every `activeProjectRoot` change; green pill when present, amber click-to-setup when absent,
  inline error text when the trigger fails. `null` (browser build / no project) renders nothing.
  Theme verified against the spec: `bg-[#121620]/60 border-white/10 text-xs rounded-lg p-2.5`
  → computed 8px radius, 10px padding, 12px font.
- **Two defects caught by verifying:**
  1. **Test flake.** Cargo runs a target's tests in parallel threads of one process, so the
     `REPOGRAPH_NO_SCAFFOLD` var set by the opt-out test was visible to the new command tests and
     could suppress scaffolding underneath them. Added `agent_scaffold::ENV_LOCK` and took it in
     every env-touching test. This would have been an intermittent CI failure, not a visible bug.
  2. **Layout.** The footer's `space-y-3` applies `margin-bottom` to `:not(:last-child)`, and the
     button's computed margin-bottom was 0 once my card became the last child — so the card
     rendered flush against it (gap 0px) while meter->button was 12px. Added explicit `mt-3`;
     both gaps now measure 12px.
- **UI verification method:** the browser pane has no Tauri host, so `window.__TAURI__` was
  stubbed with the same command contract and the component driven through every state — missing
  → click → `trigger_agent_scaffold` → `check_agent_scaffold` → green pill; project swap reverting
  to missing; a rejected trigger surfacing "permission denied" while staying amber; and recovery
  on retry. **The real Rust commands over live IPC are not verified this way** — they are covered
  by 4 unit tests, and the native window remains a manual check.
- **Verified:** `cargo test` **88 passed / 0 failed** (74 lib + 11 bin + 2 integration + 1
  route-binding); `npm run build` clean.

## CEPA Interactive Guide Modal (2026-07-22)
- **`src/components/CEPAUserGuideModal.tsx`** — glassmorphic guide: Orient / Target / Explore
  leanly steps with their real tool calls, a payoff card for `signature_only` (1,473 chars,
  98.56%), and two collapsible copy cards. Aesthetics verified from computed styles: 672px
  (max-w-2xl), 16px radius, 24px padding, `#0F1218`/0.95, `blur(24px)`, z-60 above other overlays,
  internal scroll so it fits a short viewport.
- **Trigger bug in the brief, fixed.** It specified auto-open on `status === 'updated'`, but
  `openProject`/`selectProject` both settle on `'synced'` — `'updated'` is set only by the
  `graph_updated` watcher event. As written the guide would have *never* appeared on a fresh
  project open, only after the user happened to edit a file later. Now fires on
  `'synced' || 'updated'`, guarded by an `offeredFor` ref so it prompts once per project rather
  than on every re-index.
- **Toolbar decoupling:** the "CEPA Guide" button dispatches a `repograph:open-cepa-guide` window
  event that `App.tsx` listens for, so `TopToolbar` holds no modal state and needs no prop drilling.
- **Prompt text proven identical to the source doc by SHA-256**, not by eye: minimal 824 chars
  `fa1f2ef611c54940`, architecture 398 chars `2aad8bb453f34fec` — matching
  `docs/AGENT_PROMPT_ARCHITECTURE.md` §3.1/§3.2. Two copies of the same prompt drifting apart is
  exactly the failure this guide exists to prevent, so it is worth checking mechanically.
- **Clipboard honesty:** the browser pane denied `writeText` ("Document is not focused") and the
  component surfaced "Copy failed: …" instead of a false green tick — the failure path is
  verified, not just the happy path. With the clipboard granted, the tick renders emerald-300 and
  reverts after 1.6s, and each card copies its own body.
- **Dismissal is revocable:** ticking writes `repograph_cepa_dismissed=true` and suppresses the
  next project's auto-open; re-opening from the toolbar shows the checkbox in its stored state,
  and un-ticking removes the flag. Esc and backdrop clicks close; clicks inside the panel do not.
- **Dev-CSS red herring worth remembering:** the emerald "Copied!" styling first measured as
  white/60 because the utilities were missing from the *dev server's* generated stylesheet for a
  newly created file. They were present in the production bundle all along, and a hard reload
  fixed dev. A computed-style check immediately after creating a component can lie.
- **Verified:** `npm run build` clean; `cargo test` 88 passed / 0 failed (no Rust changed).
  **Not verified:** the native Tauri window — screenshots of the browser pane timed out
  repeatedly this session, so the visual check is DOM/computed-style based, and `npm run tauri dev`
  remains a manual pass.

## Production Release, Final Verification, & Workspace Cleanup (2026-07-22)
- **Production Compilation**: Executed `cargo build --release` inside `src-tauri/` to compile the optimized production release of the Tauri/Rust backend binary `mcp_server.exe` (57.77s build time).
- **Cargo Test Suite**: Ran the full Rust unit and integration test suite `cargo test --all-targets` in `src-tauri/`, confirming all **88/88 tests** passed cleanly.
- **Frontend Asset Compilation**: Executed `npm run build` in the workspace root, compiling the frontend assets into production chunks (`dist/index.html`, CSS, and JS) cleanly in 4.64 seconds.
- **Scaffold & Workspace Cleanup**: Cleaned up the workspace root by deleting the temporary `scratch/` directory (which contained `gap_audit_checklist.md` and `telemetry_call.json`) to keep the repository footprint clean.
- **Master Walkthrough**: Wrote a consolidated master `walkthrough.md` file at the workspace root detailing core features, 5-tier context token savings audit (highlighting the **98.56% savings** milestone), rendering frame rates, passing test counts, and `REPOGRAPH_NO_SCAFFOLD=1` documentation.

## Windows Tauri Installer Build & Global MCP Registration (2026-07-22)
- **Tauri Bundle Compilation**: Executed `npm run tauri build` after enabling bundle building and adding `icons/icon.ico` to the bundle icon array in `tauri.conf.json`, generating both an `.msi` and `.exe` setup bundle.
- **Installer Verification**: Confirmed creation of `mcp_server_0.1.0_x64_en-US.msi` (3,104,768 bytes) at `C:\My-pro\project-map\src-tauri\target\release\bundle\msi\`.
- **Global MCP Configuration**: Configured the release standalone `mcp_server.exe` in both Claude Desktop (`claude_desktop_config.json`) and Codex Desktop (`config.toml`) with the full `REPOGRAPH_MCP_TOOLS` list to complete deployment.

## Walker Skip Filter Optimization (2026-07-22)
- **Directory Exclusions**: Excluded all agent workspace metadata folders (`.myrepograph-agent`, `my-agent`, `.agents`) from the repo walk scanner by adding them to `SKIP_DIRS` in `src-tauri/src/walker.rs`.
- **Watcher Integration**: Verified that the file watcher (inheriting `SKIP_DIRS`) now successfully ignores edits in `my-agent/` and `.myrepograph-agent/` memory directories, avoiding catch-up re-indexing loops on startup.
- **Verification**: Ran `cargo test` (all 88 tests passed) and rebuilt the Tauri bundles (`npm run tauri build`) successfully.




## 10k-File Scale Optimizations: DB Symbol Map, Lazy Reads, Canvas Auto-Collapse (2026-07-22)
- **Symbol map re-keying (`db.rs::populate_db`)**: `symbol_map` refactored from `HashMap<(String, String), i64>` to nested `HashMap<String, HashMap<String, i64>>` with a shared `lookup_symbol()` helper. Every probe in the reference/route-handler/inheritance/useContext/EventEmitter resolution loops previously allocated a fresh `(String, String)` tuple (two heap clones per probe); all now borrow `&str` keys with zero allocation. On a 10k-file repo the reference loop alone probes the map O(refs x imports) times, so this removes millions of transient allocations per full index.
- **Lazy file reads (`db.rs::populate_db`)**: `std::fs::read_to_string` is now gated on `!pf.extraction.symbols.is_empty()` — content is only consumed by the per-symbol FTS slices, so metadata/asset/symbol-less files skip the disk read entirely.
- **Canvas auto-collapse (`src/store.ts`)**: new `AUTO_COLLAPSE_NODE_LIMIT = 1000` + `autoCollapsedDirs(graph)` — when `graph.nodes.length > 1000`, `collapsedDirs` is pre-populated with every ancestor directory of every node path. `buildFlow`'s outermost-collapsed-ancestor rule then renders one folder node per top-level directory instead of 10k+ file/symbol DOM elements. Applied in `openProject`/`selectProject` (fresh open) and `load` (startup) — but `load` preserves a non-empty `collapsedDirs` so `graph_updated` reloads keep the user's expansions.
- **Benchmarks (honest scope)**: before/after wall-clock on a real 10,000-file repo was NOT measured — this workspace repo is ~140 files, where populate_db is already sub-second and allocation deltas are below timing noise. Recorded gains are structural: per-probe allocations 2 String clones -> 0; disk reads = all files -> only files with symbols; initial canvas DOM = O(files + symbols) -> O(top-level dirs) past 1,000 nodes.
- **Verification**: `cargo test --all-targets` **88/88 passed** (74 lib + 11 bin + 2 + 1 integration); `npm run build` clean (tsc + vite, 4.2s). Note: the test run first failed to link because a stale `target/debug/mcp_server.exe` (PID 29936) held a file lock — killed and re-ran, same trap as the 20-07 session.

## Real-Time Indexing Progress HUD (2026-07-22)
- **Backend telemetry (`indexer.rs`)**: `index_repo` now takes `app_handle: Option<&tauri::AppHandle>` and emits an `index_progress` event through four phases (`walking` -> `parsing` -> `db_write` -> `complete`). `parse_all` tracks `files_processed`/`bytes_processed` via `AtomicUsize`/`AtomicU64` updated by the existing worker pool, plus one added monitor thread inside the same `thread::scope` that polls every 100ms and emits — workers themselves never touch IPC, so parse throughput is unaffected by how often the UI updates.
- **Signature deviation, and why:** the brief specified `app_handle: &tauri::AppHandle` (non-optional). Made it `Option<&tauri::AppHandle>` instead — a bare `AppHandle` can only be constructed after `tauri::Builder` has actually started an app, and `index_repo` is called from 5 places with no such handle: the `mcp_server` CLI binary, `db::reconcile_repo_startup` (background watcher catch-up), `indexer::build_graph_for` (test helper), and two integration tests. A non-optional handle would not compile at any of them. All 5 now pass `None` (no-op emit); only `main.rs::index_and_load_graph` passes `Some(&app_handle)`.
- **Frontend (`store.ts`/`App.tsx`/`ProjectHubDashboard.tsx`)**: `IndexProgress` state + `setIndexProgress()` computing an EMA-smoothed (alpha=0.3) files/sec from the delta since the *previous* event (not a cumulative average since indexing start) so a mid-run slowdown surfaces within a poll or two instead of being diluted. `App.tsx` wires the `index_progress` Tauri listener next to the existing `graph_updated`/`agent_query_event` ones. `ProjectHubDashboard.tsx` replaces the bare spinner with a glassmorphism card: progress bar, phase label, "N / M files — X MB / Y MB" status line, and an ETA line gated to the `parsing` phase (the `walking`/`db_write` phases are near-instant single-shot steps with no meaningful ETA).
- **Verification**: `cargo test --all-targets` 88/88 passed; `npm run build` clean. Live-verified in the browser pane by driving `window.__REPO_GRAPH_STORE__.getState().setIndexProgress(...)` directly — confirmed 53% parsing state renders exactly `6,500 / 12,300 files — 24.5 MB / 56.2 MB` / `Remaining: ~1m 52s (~52 files/sec)` (EMA computed 51.7 files/sec off a real hand-fed delta, not a canned number), the 100% `db_write` state correctly omits the ETA line, and `isIndexing: false` correctly restores the original hub CTA. **Not verified:** the actual Rust `emit_all` -> JS `listen` round trip end-to-end, since the browser pane has no Tauri host to receive real IPC events (same limitation documented in the Scaffolding Status UI session) — confirmed instead that `emit_all` is called with the same pattern already proven live for `agent_query_event`.

## Global MCP Executable Path Fix (2026-07-22)
- **Root cause confirmed**: `LeftSidebar.tsx`'s "Integrate Agent" modal built `${activeProjectRoot}/src-tauri/target/debug/mcp_server.exe` — correct only when the opened project IS the Repo Graph checkout. For a real target project (`mifos_backend`, no `src-tauri/` of its own) that path never exists. `.codex/config.example.toml` had the identical assumption baked into its template.
- **Note on the brief's Retrieved Facts**: it named `CEPAUserGuideModal.tsx` as the broken component and `agent_scaffold.rs` as needing an update. Neither matched reality — grepped both for every plausible marker (`mcpServers`, `mcp_server.exe`, `claude_desktop_config`, `src-tauri`, `target/debug`) with zero hits in either file. `LeftSidebar.tsx` is the actual (and only) component with an MCP snippet in the whole `src/` tree. Fixed the real file, left the named-but-uninvolved ones alone.
- **`main.rs`**: new `get_mcp_config_snippet(project_root, app_handle)` IPC command + `resolve_mcp_binary()`. Resolution is anchored to *this running process* — `current_exe()`'s parent directory first (true for `cargo run`/dev, and for a packaged install once bundled), then Tauri's `path_resolver().resource_dir()` as a second candidate — never to the caller-supplied `project_root`, since the binary's location has nothing to do with which project is open. Returns `binary_exists` honestly (checked via `.is_file()`) alongside pre-rendered snippets for Claude Desktop (`mcpServers`), Codex (`[mcp_servers.repo-graph]` TOML), and VS Code (`{"servers": {...}}` — the real VS Code MCP schema, distinct from Claude's `mcpServers` key).
- **`tauri.conf.json`**: added `bundle.resources: ["target/release/mcp_server.exe"]` so the installer actually ships the second binary — previously `npm run tauri build` only bundled the main Tauri exe.
- **`LeftSidebar.tsx`**: the modal now fetches the snippet from the backend on open instead of computing a guessed path client-side; shows a spinner while resolving, a ✓/⚠️ detection banner (with a build hint on ⚠️), and all 3 host configs + a CLI one-liner built from the real returned path. Deleted the now-dead `mcpBinaryPath`/`mcpRootArg`/`mcpBinaryName`/`ALL_MCP_TOOLS` helpers.
- **`.codex/config.example.toml`**: rewrote the header — it was conflating "where the Repo Graph binary lives" (`command`, fixed per machine) with "which project to analyze" (`args`, the target project), the exact same bug as the UI. Now names both explicitly and points at the in-app generator as the preferred source of truth.
- **Verification**: `cargo test --all-targets` 88/88 passed; `npm run build` clean; ran `npm run tauri build` and inspected the *generated* `main.wxs`/`installer.nsi` rather than performing a real system install (installing onto this dev machine would leave behind state for a repo not meant to be installed here) — both installers place `mcp_server.exe` directly beside `Repo Graph.exe` in the install root, confirming the priority-1 `current_exe()`-sibling resolution will find it on a real install, not just in a dev checkout where the coincidence of both binaries already sharing `target/release/` could have masked a resolution bug. Live-verified the UI/store contract in the browser pane via a stubbed `window.__TAURI__.invoke` returning Rust-shaped output: confirmed the modal correctly separates the machine-global binary path from the target project's root in every generated snippet (the exact bug), and confirmed the `⚠️ Binary Missing` fallback state. **Not verified**: the real `invoke` → Rust IPC round trip (no Tauri host in the browser pane) and an actual installed-app run.

# Context Task Checklist - React Flow Virtualization & Performance Tuning

- [x] Add `onlyRenderVisibleElements={true}` to `<ReactFlow>` in `src/components/GraphCanvas.tsx`.
- [x] Run `npm run build` to verify the frontend compiles successfully without errors.
- [x] Verify that DOM render counts and zoom transitions are smooth.

# Context Task Checklist - Design System Guidelines Consolidation

- [x] Identify the 15 direct-reference files in `docs/guidlines/` and inspect `my-agent/RULES.md`.
- [x] Extract concrete tokens, component guidance, motion, and accessibility requirements.
- [x] Create `docs/UI_UX_DESIGN_SYSTEM.md` and verify its required sections.
- [x] Register the fenced design-system rule in `my-agent/RULES.md`.
- [x] Record close-out in `my-agent/memory/runtime/dailylog.md`.

**Close-out:** Created and structurally verified `docs/UI_UX_DESIGN_SYSTEM.md` (174 lines) and verified the exact design-system marker in `my-agent/RULES.md`.

# Context Task Checklist - Guideline Context Engineering Integration

- [x] Identify all 15 files in `docs/guidlines/` for full review.
- [x] Read each guideline in full and map its high-signal facts to agent retrieval.
- [x] Add topic-specific context-engineering instructions to every guideline file.
- [x] Verify all files contain the new agent guidance and record close-out.

**Close-out:** Added and verified the `Agent context engineering` section in all 15 guideline files. Each section defines the seven-piece context stack plus topic-specific Select, Write/compress, and Isolate/output behaviour.

# Context Task Checklist - Premium HIG Agent Directives

- [x] Reassess the prior context-engineering-only additions against the user’s premium-UI goal.
- [x] Add prominent, topic-specific Apple HIG application directives to every guideline source file.
- [x] Verify every directive gives agents implementation decisions rather than generic context-process advice.
- [x] Record close-out in the daily log.

**Close-out:** All 15 copied Apple HIG files now begin with a domain-specific `Agent application directive — premium UI`; `my-agent/RULES.md` now requires agents to consult the relevant original HIG file for domain decisions.

# Context Task Checklist - Full HIG Read and Master Reorganization

- [x] Read/validate all 15 non-empty local Apple HIG copies (239,000 characters).
- [x] Rebuild `docs/UI_UX_DESIGN_SYSTEM.md` into an 11-section source-traceable agent reference.
- [x] Verify source directives, traceability map, and master sections.
- [x] Record close-out in the daily log.

**Close-out:** The master now preserves concrete HIG rules while keeping agent retrieval selective.

# Context Task Checklist - Ground-Up UI/UX Redesign

- [x] `src/lib/layout.ts`: COLUMN_GAP 340, ROW_GAP 96, 44px symbol step (already 44).
- [ ] `src/index.css`: canvas #07080B, panel #0F1218, grid dots #161B26, 5px scrollbars (thumb white/12, pill radius).
- [ ] `TopToolbar.tsx`: h-12, bg #0B0D12/90, violet→cyan logo gradient, w-80 h-8 search (#141822), keep ⌘K/pills/status.
- [ ] `LeftSidebar.tsx`: default width 288, solid violet-600 h-8 folder button, segmented tabs Explorer/Routes/Context with underline, Token Meter pinned bottom.
- [ ] `DetailSidebar.tsx`: bg #0D1016/95, empty-state copy per spec.
- [ ] `GraphCanvas.tsx`: canvas bg #07080B, dots #161B26, panel surfaces #0F1218.
- [ ] Node cards: file rounded-xl + violet hover glow; symbol kind palettes (emerald #0F241B / cyan #0F2028 / purple #1A1528); folder rounded-xl.
- [ ] `EdgeHighlight.tsx`: tooltip surfaces #0F1218.
- [x] Verify: `npm run build`, then run app and visually check spacing.
- [x] Close-out: update dailylog.md.

**Close-out:** All redesign targets applied; `npm run build` clean. Verified in-browser via a temporary DEV-only gate bypass in `App.tsx` (since the 3-pane view requires `activeProjectRoot`, which only Tauri sets) — bypass reverted afterwards and rebuilt clean.

# Context Task Checklist - Agent Context Architecture Doc Audit

- [x] Audit `docs/AGENT_CONTEXT_ARCHITECTURE.md` against `src-tauri/src/mcp_server.rs`.
- [x] Fix fabricated `init_db` signature in the Layer 3 example payload.
- [x] Correct `repograph_status` / `repograph_search` descriptions to match registration code.
- [x] Convert Layer 4 tool schemas from `parameters` to the MCP wire key `inputSchema`.
- [x] Live-call the 5 previously untested tools and document their real response formats.
- [x] Record the non-uniform JSON/text response matrix and retrieval caveats.

**Close-out:** Only `repograph_explore` and `repograph_search` return JSON; the other six return plain text or raw source. `repograph_search` returns the full source of every match (expensive). `repograph_callees` includes in-function local variables alongside real calls.

# Context Task Checklist - MCP Rebuild & Edge-Case Doc Hardening

- [x] Rebuild `mcp_server.exe` — required killing stale PID 38088 (started 20-07 22:06) still holding the lock.
- [x] Re-run `cargo test --lib` → 36 passed, 0 failed.
- [x] Verify corrected manifest footer over stdio JSON-RPC (`repograph_files`).
- [x] Update `docs/BENCHMARK_REPORT.md` (new §5 counter-case + cache warning).
- [x] Update `docs/PLAYBOOK.md` (new §26 retrieval semantics).
- [x] Add path-disambiguation rules to `my-agent/RULES.md` (new §7) + fix stale `read_file` reference in §6.
- [x] Close-out: update dailylog.md.

**Close-out:** Footer fix verified live over stdio. Root-caused an indefinite startup hang to a **664 MB `.repograph/graph.db`** (136-file repo; `graph.json` is 448 KB) — bloated DB hung >120 s with zero output, fresh DB completed the identical round-trip in **90 ms**. Bloated file renamed to `graph.db.bloated-664mb.bak`, not deleted.

# Context Task Checklist - Open-Source Readiness & Parser Gap

- [x] Audit `src-tauri/src/parsers` + `walker.rs::language_for` dispatch for coverage gaps.
- [x] Add `parsers/sfc.rs` — Vue/Svelte SFC extractor delegating to `JsExtractor` with line-offset correction.
- [x] Close extension gaps: `mts/cts`, `vue/svelte`, `kts`, `cc/cxx/hh/hxx`, `htm`, `mdx`, `Dockerfile.*`, `*.dockerfile`.
- [x] `.gitignore`: exclude `.repograph/`, `src-tauri/target`, `*.bak`, `.codex/config.toml`.
- [x] Replace hardcoded author paths in the in-app MCP integration modal with platform-aware placeholders + `REPOGRAPH_MCP_TOOLS`.
- [x] Ship `.codex/config.example.toml` instead of a machine-specific committed config.
- [x] Verify: `cargo test` 42 passed / 0 failed; `npm run build` clean; binary rebuilt.

# Context Task Checklist - State Store & Route Extraction

**Audit first:** routes are ALREADY implemented in all 8 extractors (route_emit_sites 1-6 each;
`scan_go_routes`, `scan_rust_routes`, `scan_router_calls`, `scan_annotation_routes`, Ktor/Symfony/
Micronaut/Remix/Next tests all present). `db.rs:407` synthesises `kind="route"` symbols from
`entry_points`. Re-implementing = duplication + regression risk. `state_store` = 0 hits anywhere → the real gap.

- [x] New `parsers/state.rs` — shared line-scanner (mirrors `routes.rs`), import-gated to cut false positives.
- [x] JS/TS: Zustand `create`, Redux `createSlice`/`configureStore`, React `createContext`, Pinia `defineStore`, Vuex `createStore`, MobX `makeAutoObservable`, Recoil/Jotai `atom`.
- [x] Python: Celery `@app.task`/`@shared_task`, module-level `Celery(`/`Redis(` singletons.
- [x] Rust: `mpsc/broadcast/watch::channel`, `static`/`OnceLock`/`lazy_static` globals.
- [x] Go: `make(chan ...)` bindings.
- [x] Re-tag existing AST symbols (preserve real line ranges) — verified: `useGraphStore` kept L86-401.
- [x] `sfc.rs` inherits JS state detection automatically (delegates to JsExtractor).
- [x] Frontend: amber/gold theme + `STR` badge in `CustomSymbolNode.tsx`; dotted gold edge + tooltip in `EdgeHighlight.tsx`.
- [x] Verify `cargo test` (53 pass) + `npm run build` (clean) + real re-index.
- [x] **Skipped deliberately:** `db.rs` state-usage edge synthesis — 135 edges already point into state stores via the existing reference/call mechanism; a second pass would duplicate every one of them.

**Close-out:** First real-repo index exposed 2 false positives from my own scanner (a `//` comment and a test string literal in `state.rs` both matched `mpsc::channel`), plus a tuple binding reported as one symbol `"tx, rx"`. Fixed: skip comments, reject string-literal RHS, split destructured bindings. Re-index now yields 6 genuine stores, 0 false positives.

# Context Task Checklist - symbols_fts Growth Bug

- [x] Trace every `symbols_fts` write path.
- [x] ~~Hypothesis: FK cascade doesn't fire the delete trigger~~ **DISPROVEN** — regression test passed with and without the "fix"; the cascade does fire the trigger.
- [x] Query the archived 664 MB DB directly: row counts normal (137 files / 4,374 FTS rows), so never a duplicate-row bug.
- [x] `dbstat` → `symbols_fts_content` = 470 MB / 4,374 rows (~110 KB each); largest rows all single-letter symbols in `dist/assets/index-<hash>.js`.
- [x] **Real cause:** `watcher.rs::is_supported_file` had a private denylist (`.git`, `node_modules`, `.repograph`) missing `dist`/`build`/`target`/`out`/`.next`/`coverage`, so every `npm run build` re-indexed the minified bundle.
- [x] Fix: watcher delegates to `walker::SKIP_DIRS` + `walker::language_for` + `parsers::extractor_for`; deleted the duplicated 7-extension language map.
- [x] Reverted the redundant `DELETE FROM symbols_fts` added on the disproven theory.
- [x] Removed the machine-specific diagnostic test before shipping.
- [x] Verify: 45 tests pass; fresh index 139 files / 343 ms / **0.37 MB** (was 664 MB).

**Close-out:** The row-count regression test could never have caught this — the bug was content size, not row count. Confirmed only by querying the archived DB with `dbstat`. Docs corrected: earlier entries asserting FTS row accumulation were wrong.

**Close-out (OSS readiness):** Biggest OSS trap was config-generation, not code — every generated snippet omitted `REPOGRAPH_MCP_TOOLS`, so new users saw 1 tool and assumed breakage. Left the explore-only server default intact (deliberate §25 design) and fixed the generators instead. **Unfixed:** `symbols_fts` unbounded growth (the 664 MB hang) — still the top remaining OSS risk.

# Context Task Checklist - Landing Routing & 1,000+ File Canvas Performance

- [x] `store.ts::load` no longer restores `activeProjectRoot` from the backend, so a cold
      start always lands on `ProjectHubDashboard` (a `graph_updated` reload keeps the open project).
- [x] `store.ts::goHome` + Home button (`<Home size={15} />`) in `TopToolbar.tsx`.
- [x] O(1) hover: `hoverHighlight.ts` rewrites one generated stylesheet instead of
      re-running 6 zustand selectors x N nodes per mouse move.
- [x] `CustomFileNode.tsx` 6 subscriptions -> 1 `useShallow` selector; hover/dim state removed entirely.
- [x] `EdgeHighlight.tsx` no longer subscribes to `hoveredPath`; exposes
      `data-edge-source` / `data-edge-target` for the generated rules.
- [x] `buildFlow` bakes `|a|b|` adjacency into node data (`data-node-neighbors`).
- [x] `onlyRenderVisibleElements={true}` (already present) + progressive ingest overlay >500 nodes.
- [x] Verify: `npm run build` clean; measured in-browser with a synthetic 1,200-node / 6,000-edge graph.

**Measured (Vite dev build, browser pane, 1,200-node synthetic graph):**
| mounted nodes | hover move->move (median) | hover enter-from-idle (median) |
|---|---|---|
| 60  | 2.3 ms  | 18 ms |
| 150 | 2.9 ms  | 37 ms |
| 400 | 4.7 ms  | 88 ms |
| 1200 (fit-whole-repo) | 11.7 ms | ~200 ms |

Correctness: hovering lit exactly 11 nodes (self + 10 neighbours) and dimmed 1,189; zero React renders.

**Two traps found by measuring rather than assuming:**
1. Toggling `data-hovered-path` on the canvas *container* (the shape the spec asked for)
   invalidates the whole 18k-element subtree: **98.8 ms per hover**. The dim rules had to move
   into the generated stylesheet so nothing on an ancestor changes. Measured alternatives:
   container attribute 98.8 ms vs CSSOM rule swap 7.8 ms vs unrelated attribute 0 ms.
2. `will-change: opacity` on the node class would promote one compositor layer per node
   (1,200 layers). Removed.

**Not verified:** frame times under a real 60 FPS paint loop. The browser pane runs the page
hidden, so rAF is frozen and `setTimeout` is clamped to ~1 s — frame-time and
virtualisation-under-pan numbers cannot be trusted from this environment. The table above is
forced-style-recalc cost per hover, which is the dominant term but not a frame time.
Enter-from-idle inherently recalculates every mounted node (they all change opacity);
virtualisation is what keeps that number small at working zoom.

# Context Task Checklist - State Store MCP Cost Audit & Visual Verification

- [x] Read `src-tauri/src/parsers/state.rs` (326 lines) — Zustand rule is `("create", mentions("zustand"))`, import-gated.
- [x] Drive `mcp_server.exe` over stdio JSON-RPC and byte-count every payload
      (`scratchpad/mcp_audit.mjs`, `REPOGRAPH_MCP_TOOLS=all`).
- [x] Arm A baseline: 7 files = 98,574 chars / 26,642 tokens / $0.079926.
- [x] Arm B measured (exact bytes from the wire):
      - `repograph_explore(["useGraphStore"])` = **26,891** chars (files 10,785 + paths 16,087; 138 edges)
      - `repograph_node(src/store.ts, useGraphStore)` = **10,316** chars
      - `repograph_callers(useGraphStore)` = **7,499** chars
      - `repograph_search("useGraphStore")` = 10,723 chars (returns full source of matches)
      - `repograph_search("store")` = **2 chars (`[]`)** — see defect below
- [x] Savings: explore-only **72.7%**, node+callers **81.9%**, node-only 89.5%. **Target >90% NOT met.**
- [x] Visual verification in the live app (browser pane on the Tauri dev server, port 5173).

**Defect found — FTS is prefix-only, not substring:**
`repograph_search("store")` and `("store*")` both return `[]` even though `useGraphStore`
is indexed; `("useGraph")` returns 10,723 chars. FTS5 matches a *prefix of the identifier
token*, so the obvious lowercase query for a store returns nothing. The task's suggested
`repograph_search(query: "store")` would have found nothing.

**Why the >90% target misses here:** `useGraphStore` spans L92-460 of a 479-line file —
**69% of `src/store.ts`** — which is the ">70% of file" band where `BENCHMARK_REPORT.md` §5
already says to prefer `repograph_node` over `explore`. On top of that, `paths` is **60% of
the explore payload** (16,087 of 26,891 chars) for 138 edges, of which 12 are file-level
pseudo-edges (`src/App.tsx#src/App.tsx`) and most of the rest are in-function locals
(`#load`, `#activeProjectRoot`, `#unsubscribeGraph`). The genuinely useful content — the
12 distinct subscriber files — is ~400 chars.

**Visual verification (measured from computed styles, not eyeballed):**
- `src/store.ts#useGraphStore` renders with `border-amber-500/40 bg-[#261d10]/90 text-amber-300`,
  computed colour `oklch(0.879 0.169 91.605)` (amber-300), `STR` badge at font-weight 700, uppercase.
- All 13 mounted edges into `useGraphStore` render `rgb(245, 158, 11)` (#F59E0B), dash `2, 3`,
  width 1.6 — including `GraphCanvas.tsx#graph` and `CustomFileNode.tsx#CustomFileNode`.
- Hover card reads exactly `State Store Usage` + `CustomFileNode → useGraphStore`,
  amber-400 heading, amber/30 border, #0F1218/95 surface.
- Symbol edges only mount when **both** endpoint symbol nodes are on screen — with
  `onlyRenderVisibleElements` they vanish at close zoom. Not a bug; worth knowing when verifying.

# Context Task Checklist - FTS5 Tokenizer, Explore Pruning, Multi-Stack Parsers

- [x] `db.rs`: `tokens` column on `symbols_fts` + `tokenize_symbol_name` (camelCase, snake_case, acronyms).
      FTS5 has no ALTER, so a 3-column table is detected as stale and rebuilt on `init_db`.
- [x] `db.rs::build_fts_match_query` shared by `mcp_server.rs` and the `main.rs` Tauri command.
- [x] `mcp_server.rs::collapse_endpoints`: named symbols win; a file contributing only locals
      collapses to ONE `references` edge; same-file noise dropped, same-file real symbols kept.
- [x] `parsers/schema.rs` (+ wired into js/python/rust/go/java/csharp/php, new GraphQL SDL extractor,
      Prisma models now emit symbols instead of exports-only).
- [x] `CustomSymbolNode.tsx`: indigo `DB`, rose `EVT`.
- [x] Verify: `cargo test` 69 passed / 0 failed; re-index; stdio byte-count audit re-run.

**Known before starting (arithmetic, not opinion):** the `<8,000 char` explore target is
unreachable for `useGraphStore` — its source slice alone is 10,316 chars, so even zero `paths`
leaves ~10.8k. Likewise >90% savings needs <=9,857 chars against the 26,642-token baseline.
Pruning fixes the `paths` bloat (16,087 chars); it cannot shrink the code the tool exists to return.

**Measured after (stdio byte-count, fresh index):**
| call | before | after |
| :--- | ---: | ---: |
| `repograph_search("store")` | 2 chars (`[]`) | 10,874 chars → `StateStore`, `useGraphStore` |
| `repograph_explore(["useGraphStore"])` | 26,891 | **12,679** |
| ├ `files` (source slice) | 10,785 | 10,785 (unchanged — this is the answer) |
| └ `paths` | 16,087 (138 edges) | **1,875 (17 edges)** |
Savings vs 100,180-char baseline: 73.2% → **87.3%**.

**Targets NOT met, and why (arithmetic):** `<8,000 chars` and `>90%` are unreachable for this
symbol. `useGraphStore`'s source slice alone is 10,785 chars; >90% of a 27,076-token baseline
needs <=9,857 chars total. Pruning removed 88% of `paths` (all that was available to remove).
Reaching >90% requires a *signature-only* explore mode, not more pruning.

**False positive caught before shipping:** the first `scan_rust` run tagged `posts` and `Post`
in `schema.rs` itself — it read its own `r#"..."#` test fixtures as Diesel code. Added raw-string
and Python-docstring guards plus two regression tests. Post-fix repo scan: 2 `database_schema`
(the genuine Prisma fixture models), 0 false positives, 0 `event_channel` (none exist here).

# Context Task Checklist - Badge Alignment & signature_only Explore

- [x] `DetailSidebar.tsx`: `KIND_BADGES` + `<KindBadge>` replacing `kind.substring(0,3)` at all
      3 call sites (symbol list, callers, callees). DB indigo / EVT rose / STR amber, plus
      API emerald and CMP cyan to match `CustomSymbolNode.tsx`. Unknown kinds keep the truncation.
- [x] `mcp_server.rs`: `signature_only` boolean in the explore inputSchema + `tools_call` parsing.
- [x] `mcp_server.rs::signature_block` — declaration head only, stops at the body opener
      (`{`, `:`, `=>`, `;`), capped at `MAX_SIGNATURE_LINES = 6`, trailing brace stripped.
- [x] Verify: `cargo test --lib` 72 passed / 0 failed; `npm run build` clean; stdio audit re-run.

**Measured (stdio byte-count, baseline = 102,193 chars / 27,620 tokens):**
| explore mode | chars | tokens | USD | savings |
| :--- | ---: | ---: | ---: | ---: |
| original (pre-pruning) | 26,891 | 7,268 | $0.021804 | 73.69% |
| pruned paths | 12,679 | 3,427 | $0.010281 | 87.59% |
| **`signature_only: true`** | **2,085** | **564** | **$0.001692** | **97.96%** |
| `signature_only` on `init_db` | 768 | 208 | — | — |

Returned signature for `init_db`: `pub fn init_db(db_path: &Path) -> Result<Connection>`.

**Targets narrowly missed: 2,085 chars (target <1,500) and 97.96% (target >98%).**
Composition: `files` 191 chars, `paths` **1,875 chars / 17 edges** = 90% of the payload — and the
edges are the answer to the architecture question, not overhead. Of that, **748 chars are repeated
JSON keys** (`from_symbol`/`to_symbol`/`kind` per edge). Rendering edges as compact strings
measures at **1,473 chars / 98.56%** — clears both targets, but it changes `ExplorePayload`'s wire
format, which CLAUDE.md §5 makes a breaking change requiring a schema-version bump. Not done
unrequested; it is the one remaining lever.

# Context Task Checklist - Schema Version Bump & Compact Edge Serialization

- [x] `manifest.rs`: `MANIFEST_SCHEMA_VERSION = "1.1.0"` (semver String) + rendered into the
      manifest markdown footer — `Manifest` is never serialized to an agent, so a version bump
      that only lived in the struct would have been invisible. Golden render test updated.
- [x] `graph.rs`: `Graph::schema_version` deliberately left `u32 = 1`. The on-disk cache layout
      did not change; making it a string would fail to deserialize every existing
      `.repograph/graph.json` and contradict `src/types.ts` (`schema_version: number`).
- [x] `mcp_server.rs`: `compact_edge()` + `compact_edges` param defaulting to `signature_only`.
- [x] Verify: `cargo test` 77 across all targets, 0 failed; stdio audit.

**Final audit (baseline 102,193 chars / 27,620 tokens):**
| mode | chars | tokens | USD | savings |
| :--- | ---: | ---: | ---: | ---: |
| original (no pruning) | 26,891 | 7,268 | $0.021804 | 73.69% |
| pruned paths | 12,679 | 3,427 | $0.010281 | 87.59% |
| `compact_edges` only | 12,067 | 3,261 | $0.009783 | 88.19% |
| `signature_only` (verbose edges) | 2,085 | 564 | $0.001692 | 97.96% |
| **`signature_only` + `compact_edges`** | **1,473** | **398** | **$0.001194** | **98.56%** |

Target met exactly: **1,473 chars < 1,500**, **98.56% > 98.5%**. Payload = 191 chars signature
+ 1,263 chars of 17 arrow-string edges. `signature_only: true` alone also yields 1,473 (the
default kicks in); `compact_edges` alone on a full-body call still saves 612 chars.

**Deviation from the literal spec:** edges serialize as `from -kind-> to`, not `from->to`.
The `kind` separates a named caller (`calls`) from a file whose references were all local
bindings (`references`) — the distinction `collapse_endpoints` exists to preserve. It costs
~10 chars/edge and the 1,473 target was met with it included.

# Context Task Checklist - Master Documentation Finalization

- [x] `docs/BENCHMARK_REPORT.md`: §2 rewritten as the 5-tier arc + §2.1 payload breakdown +
      §2.2 compact edge syntax; §3 formula, §4 (3 -> 5 advancements), §5 counter-case note and
      the Rule-of-Thumb table all updated for v1.1.0.
- [x] `docs/PLAYBOOK.md`: new §25.3 (signature_only / compact_edges / the two version fields)
      and §25.4 (semantic kinds + badges + import gating); old §25.3 renumbered to §25.5.
- [x] `walkthrough.md`: new §1.3/§1.4, real 77-test log, 5-tier savings table, §2.3 search and
      parser verification, refreshed modified-files list.
- [x] Verify: `cargo test` 77/77 (74+0+0+2+1+0), `npm run build` clean.

**Corrections made while writing (docs asserted things that were not true):**
1. Baseline cost: the brief said $0.0799; 27,620 tokens x $3/M = **$0.082860**. The old figure
   came from the pre-edit 26,642-token baseline. Docs use the recomputed value.
2. `walkthrough.md` claimed `.repograph/graph.json` was "deprecated in favor of a pure SQLite
   pipeline". It is still written by `index_repo` and still read by the visualizer
   (IPC / `/api/graph`). Rewritten to state what each store is actually for.
3. The old 93.8% headline measured a *different query* (3 files / 16,057 tokens) and is not
   comparable to the new table. Kept as an explicitly-labelled historical note rather than
   silently overwritten, and the reporting note now says savings are per-question.

**Baseline cost discrepancy to resolve honestly:** the brief states $0.0799 for Arm A, but
27,620 tokens x $3/M = **$0.082860**. $0.0799 corresponds to the older 26,642-token baseline
measured before these files were edited. Docs use the recomputed, self-consistent figure.

# Context Task Checklist - CEPA Master Guide & Rules Registration

- [x] Read `~/.gemini/config/skills/context_engineering_prompt_builder/SKILL.md` (7-piece stack +
      Write/Select/Compress/Isolate), PLAYBOOK §26 (CEPA spec already present; the old query-
      hardening section is now §27), and the RULES.md `# X_MARKER` / `# END_X_MARKER` convention.
- [x] Created `docs/AGENT_PROMPT_ARCHITECTURE.md` — 7-piece stack mapped to concrete workspace
      artifacts with per-layer token sizes, the 3-step sequence (+ step 4 "load bodies to write"),
      4 copy-pasteable prompt headers, the 4-move context lifecycle, a "when CEPA does not apply"
      section, and a quick-reference table.
- [x] Appended `CONTEXT_ENGINEERING_PROMPT_ARCHITECTURE_MARKER` to `my-agent/RULES.md`.
- [x] Verified: 7/7 relative links resolve, 18 code fences balanced, every quoted metric
      (27,620 / 398 / 1,473 / 12,679 / 87.59% / 98.56% / 1.31x / v1.1.0) matches the measured
      values in BENCHMARK_REPORT.md; `npm run build` clean; `cargo test` 77/77.

**One addition to the specified marker text:** the brief's block says to *always* default to
`signature_only: true`. Read alone that licenses editing code from a declaration head. Added an
explicit exception line inside the same fenced block requiring a full-body re-fetch before
modifying, refactoring, or debugging behaviour — consistent with PLAYBOOK §26.2, which already
says to load bodies when modification is required.

# Context Task Checklist - Autonomous .myrepograph-agent/ Scaffolding

- [x] `src-tauri/src/agent_scaffold.rs` — `ensure_agent_scaffold`, 10 dirs + 14 template files.
- [x] `mod agent_scaffold;` in `main.rs`; called in `index_and_load_graph` after `index_repo`,
      failure logged (`[scaffold] skipped for …`) and swallowed so indexing still succeeds.
- [x] 7 unit tests: full tree, content integrity (CEPA-v1.1 + marker block + write-time
      exception), idempotence, no-overwrite under forced re-run, deleted-file restore,
      opt-out env, odd roots, and `bash -n` on the generated script.
- [x] Verify: `cargo test` **84 passed / 0 failed** (74 lib + 7 bin + 2 + 1); `npm run build` clean.

**Test-harness trap worth remembering:** on this box `bash` resolves to a WSL relay that spawns
fine and then fails `execvpe(/bin/bash)`. `Command::new("bash").arg("-n")` returned non-zero for
that reason, not for a syntax error, so the first version of the script test failed for the wrong
reason. Fixed with a `bash -c "exit 0"` probe that separates "bash works" from "script is bad".
Script syntax then verified for real against Git Bash: `bash -n review.sh` → SYNTAX OK, and the
generated `agent.yaml` indentation checked with `cat -A`.

**Safety constraints I am imposing (this writes into arbitrary user repos):**
- Never overwrite an existing file — per-file `create_new`, so user customizations survive.
- Never fail the indexing command if scaffolding fails (log and continue).
- Skip entirely when `.myrepograph-agent/` already exists (the documented trigger).
- Provide `REPOGRAPH_NO_SCAFFOLD=1` opt-out — creating ~20 files in someone's repo the first
  time they open it is a side effect they must be able to decline.

# Context Task Checklist - Scaffolding Status UI Integration

- [x] `main.rs`: `check_agent_scaffold` (read-only, never errors) + `trigger_agent_scaffold`
      (canonicalizes, rejects non-directories, reports failure); both in `generate_handler![]`
      (14 -> 16 commands).
- [x] `LeftSidebar.tsx`: `<AgentScaffoldStatus>` with `hasScaffold: boolean | null`, re-checked
      on every `activeProjectRoot` change; green pill / amber click-to-setup / inline error.
- [x] Verify: `cargo test` **88 passed / 0 failed** (74 + 11 + 2 + 1); `npm run build` clean;
      both UI states + click path + failure path exercised in the browser pane.

**Two defects found by verifying rather than assuming:**
1. **Test flake, fixed:** cargo runs a target's tests in parallel threads of ONE process, so the
   `REPOGRAPH_NO_SCAFFOLD` set by `opt_out_env_suppresses_scaffolding` was visible to the new
   command tests and could suppress scaffolding underneath them. Added `agent_scaffold::ENV_LOCK`
   and took it in every env-touching test.
2. **Layout, fixed:** the footer's `space-y-3` puts `margin-bottom` on `:not(:last-child)` only,
   and the button's computed `margin-bottom` was 0 once my card became the last child — the card
   rendered flush (gap 0px) while meter->button was 12px. Added explicit `mt-3`; both gaps are
   now 12px, measured.

**Not verified:** the real Rust commands over live IPC. The browser pane has no Tauri host, so the
UI was driven against a stubbed `window.__TAURI__` implementing the same contract; the Rust side is
covered by 4 unit tests instead. The native-window path is still a manual check.

**Path-safety note:** both new commands take a caller-supplied `project_root` string and write to
disk. `trigger_agent_scaffold` must canonicalize and reject anything that is not an existing
directory, so a malformed/hostile path cannot scatter template files somewhere unintended.

# Context Task Checklist - CEPA Interactive Guide Modal

- [x] `src/components/CEPAUserGuideModal.tsx` — 672px glass panel (max-w-2xl / rounded-2xl 16px /
      p-6 24px / bg-[#0F1218]/95 / backdrop-blur 24px / z-60), Orient-Target-Explore steps,
      the `signature_only` payoff card (1,473 chars, 98.56%), 2 collapsible copy cards.
- [x] `App.tsx` — mounted; auto-opens once per project via an `offeredFor` ref; toolbar re-open
      over a `repograph:open-cepa-guide` window event so the toolbar owns no modal state.
- [x] `TopToolbar.tsx` — `<HelpCircle size={15} />` "CEPA Guide" pill before the status badge.
- [x] Verify: `npm run build` clean, `cargo test` 88 passed; full flow exercised in-browser.

**Trigger bug in the brief, fixed:** it said auto-open on `status === 'updated'`, but
`openProject`/`selectProject` settle on `'synced'` — `'updated'` is only set by the
`graph_updated` watcher event. As written the guide would never appear on a fresh project open,
only after the user later edited a file. Now fires on `'synced' || 'updated'`, once per project.

**Verified in-browser (DOM + computed styles):** auto-open on index completion; "Copied!" tick in
emerald-300/emerald-500-15 reverting after 1.6s; each card copying its own body; dismissal writing
`repograph_cepa_dismissed=true`, suppressing the next project's auto-open, and being revocable by
un-ticking; toolbar re-open; Esc and backdrop close; inner clicks not closing.

**Prompt text proven identical to the docs by hash** — minimal 824 chars `fa1f2ef611c54940`,
architecture 398 chars `2aad8bb453f34fec`, matching `docs/AGENT_PROMPT_ARCHITECTURE.md` §3.1/§3.2.

**Dev-CSS red herring:** the emerald "Copied!" styling first measured as white/60. The utilities
were absent from the *dev server's* generated stylesheet for the newly created file; they are
present in the production bundle, and a hard reload made dev match. Not a code defect — but it is
why a computed-style check right after creating a file can lie.

**Spec bug to resolve:** the brief keys auto-open on `status === 'updated'`, but
`openProject`/`selectProject` set `'synced'` — `'updated'` is only set by the `graph_updated`
watcher event. Keying on `'updated'` means the guide NEVER appears on a fresh project open, only
**Targets NOT met, and why (arithmetic):** `<8,000 chars` and `>90%` are unreachable for this
symbol. `useGraphStore`'s source slice alone is 10,785 chars; >90% of a 27,076-token baseline
needs <=9,857 chars total. Pruning removed 88% of `paths` (all that was available to remove).
Reaching >90% requires a *signature-only* explore mode, not more pruning.

**False positive caught before shipping:** the first `scan_rust` run tagged `posts` and `Post`
in `schema.rs` itself — it read its own `r#"..."#` test fixtures as Diesel code. Added raw-string
and Python-docstring guards plus two regression tests. Post-fix repo scan: 2 `database_schema`
(the genuine Prisma fixture models), 0 false positives, 0 `event_channel` (none exist here).

# Context Task Checklist - Badge Alignment & signature_only Explore

- [x] `DetailSidebar.tsx`: `KIND_BADGES` + `<KindBadge>` replacing `kind.substring(0,3)` at all
      3 call sites (symbol list, callers, callees). DB indigo / EVT rose / STR amber, plus
      API emerald and CMP cyan to match `CustomSymbolNode.tsx`. Unknown kinds keep the truncation.
- [x] `mcp_server.rs`: `signature_only` boolean in the explore inputSchema + `tools_call` parsing.
- [x] `mcp_server.rs::signature_block` — declaration head only, stops at the body opener
      (`{`, `:`, `=>`, `;`), capped at `MAX_SIGNATURE_LINES = 6`, trailing brace stripped.
- [x] Verify: `cargo test --lib` 72 passed / 0 failed; `npm run build` clean; stdio audit re-run.

**Measured (stdio byte-count, baseline = 102,193 chars / 27,620 tokens):**
| explore mode | chars | tokens | USD | savings |
| :--- | ---: | ---: | ---: | ---: |
| original (pre-pruning) | 26,891 | 7,268 | $0.021804 | 73.69% |
| pruned paths | 12,679 | 3,427 | $0.010281 | 87.59% |
| **`signature_only: true`** | **2,085** | **564** | **$0.001692** | **97.96%** |
| `signature_only` on `init_db` | 768 | 208 | — | — |

Returned signature for `init_db`: `pub fn init_db(db_path: &Path) -> Result<Connection>`.

**Targets narrowly missed: 2,085 chars (target <1,500) and 97.96% (target >98%).**
Composition: `files` 191 chars, `paths` **1,875 chars / 17 edges** = 90% of the payload — and the
edges are the answer to the architecture question, not overhead. Of that, **748 chars are repeated
JSON keys** (`from_symbol`/`to_symbol`/`kind` per edge). Rendering edges as compact strings
measures at **1,473 chars / 98.56%** — clears both targets, but it changes `ExplorePayload`'s wire
format, which CLAUDE.md §5 makes a breaking change requiring a schema-version bump. Not done
unrequested; it is the one remaining lever.

# Context Task Checklist - Schema Version Bump & Compact Edge Serialization

- [x] `manifest.rs`: `MANIFEST_SCHEMA_VERSION = "1.1.0"` (semver String) + rendered into the
      manifest markdown footer — `Manifest` is never serialized to an agent, so a version bump
      that only lived in the struct would have been invisible. Golden render test updated.
- [x] `graph.rs`: `Graph::schema_version` deliberately left `u32 = 1`. The on-disk cache layout
      did not change; making it a string would fail to deserialize every existing
      `.repograph/graph.json` and contradict `src/types.ts` (`schema_version: number`).
- [x] `mcp_server.rs`: `compact_edge()` + `compact_edges` param defaulting to `signature_only`.
- [x] Verify: `cargo test` 77 across all targets, 0 failed; stdio audit.

**Final audit (baseline 102,193 chars / 27,620 tokens):**
| mode | chars | tokens | USD | savings |
| :--- | ---: | ---: | ---: | ---: |
| original (no pruning) | 26,891 | 7,268 | $0.021804 | 73.69% |
| pruned paths | 12,679 | 3,427 | $0.010281 | 87.59% |
| `compact_edges` only | 12,067 | 3,261 | $0.009783 | 88.19% |
| `signature_only` (verbose edges) | 2,085 | 564 | $0.001692 | 97.96% |
| **`signature_only` + `compact_edges`** | **1,473** | **398** | **$0.001194** | **98.56%** |

Target met exactly: **1,473 chars < 1,500**, **98.56% > 98.5%**. Payload = 191 chars signature
+ 1,263 chars of 17 arrow-string edges. `signature_only: true` alone also yields 1,473 (the
default kicks in); `compact_edges` alone on a full-body call still saves 612 chars.

**Deviation from the literal spec:** edges serialize as `from -kind-> to`, not `from->to`.
The `kind` separates a named caller (`calls`) from a file whose references were all local
bindings (`references`) — the distinction `collapse_endpoints` exists to preserve. It costs
~10 chars/edge and the 1,473 target was met with it included.

# Context Task Checklist - Master Documentation Finalization

- [x] `docs/BENCHMARK_REPORT.md`: §2 rewritten as the 5-tier arc + §2.1 payload breakdown +
      §2.2 compact edge syntax; §3 formula, §4 (3 -> 5 advancements), §5 counter-case note and
      the Rule-of-Thumb table all updated for v1.1.0.
- [x] `docs/PLAYBOOK.md`: new §25.3 (signature_only / compact_edges / the two version fields)
      and §25.4 (semantic kinds + badges + import gating); old §25.3 renumbered to §25.5.
- [x] `walkthrough.md`: new §1.3/§1.4, real 77-test log, 5-tier savings table, §2.3 search and
      parser verification, refreshed modified-files list.
- [x] Verify: `cargo test` 77/77 (74+0+0+2+1+0), `npm run build` clean.

**Corrections made while writing (docs asserted things that were not true):**
1. Baseline cost: the brief said $0.0799; 27,620 tokens x $3/M = **$0.082860**. The old figure
   came from the pre-edit 26,642-token baseline. Docs use the recomputed value.
2. `walkthrough.md` claimed `.repograph/graph.json` was "deprecated in favor of a pure SQLite
   pipeline". It is still written by `index_repo` and still read by the visualizer
   (IPC / `/api/graph`). Rewritten to state what each store is actually for.
3. The old 93.8% headline measured a *different query* (3 files / 16,057 tokens) and is not
   comparable to the new table. Kept as an explicitly-labelled historical note rather than
   silently overwritten, and the reporting note now says savings are per-question.

**Baseline cost discrepancy to resolve honestly:** the brief states $0.0799 for Arm A, but
27,620 tokens x $3/M = **$0.082860**. $0.0799 corresponds to the older 26,642-token baseline
measured before these files were edited. Docs use the recomputed, self-consistent figure.

# Context Task Checklist - CEPA Master Guide & Rules Registration

- [x] Read `~/.gemini/config/skills/context_engineering_prompt_builder/SKILL.md` (7-piece stack +
      Write/Select/Compress/Isolate), PLAYBOOK §26 (CEPA spec already present; the old query-
      hardening section is now §27), and the RULES.md `# X_MARKER` / `# END_X_MARKER` convention.
- [x] Created `docs/AGENT_PROMPT_ARCHITECTURE.md` — 7-piece stack mapped to concrete workspace
      artifacts with per-layer token sizes, the 3-step sequence (+ step 4 "load bodies to write"),
      4 copy-pasteable prompt headers, the 4-move context lifecycle, a "when CEPA does not apply"
      section, and a quick-reference table.
- [x] Appended `CONTEXT_ENGINEERING_PROMPT_ARCHITECTURE_MARKER` to `my-agent/RULES.md`.
- [x] Verified: 7/7 relative links resolve, 18 code fences balanced, every quoted metric
      (27,620 / 398 / 1,473 / 12,679 / 87.59% / 98.56% / 1.31x / v1.1.0) matches the measured
      values in BENCHMARK_REPORT.md; `npm run build` clean; `cargo test` 77/77.

**One addition to the specified marker text:** the brief's block says to *always* default to
`signature_only: true`. Read alone that licenses editing code from a declaration head. Added an
explicit exception line inside the same fenced block requiring a full-body re-fetch before
modifying, refactoring, or debugging behaviour — consistent with PLAYBOOK §26.2, which already
says to load bodies when modification is required.

# Context Task Checklist - Autonomous .myrepograph-agent/ Scaffolding

- [x] `src-tauri/src/agent_scaffold.rs` — `ensure_agent_scaffold`, 10 dirs + 14 template files.
- [x] `mod agent_scaffold;` in `main.rs`; called in `index_and_load_graph` after `index_repo`,
      failure logged (`[scaffold] skipped for …`) and swallowed so indexing still succeeds.
- [x] 7 unit tests: full tree, content integrity (CEPA-v1.1 + marker block + write-time
      exception), idempotence, no-overwrite under forced re-run, deleted-file restore,
      opt-out env, odd roots, and `bash -n` on the generated script.
- [x] Verify: `cargo test` **84 passed / 0 failed** (74 lib + 7 bin + 2 + 1); `npm run build` clean.

**Test-harness trap worth remembering:** on this box `bash` resolves to a WSL relay that spawns
fine and then fails `execvpe(/bin/bash)`. `Command::new("bash").arg("-n")` returned non-zero for
that reason, not for a syntax error, so the first version of the script test failed for the wrong
reason. Fixed with a `bash -c "exit 0"` probe that separates "bash works" from "script is bad".
Script syntax then verified for real against Git Bash: `bash -n review.sh` → SYNTAX OK, and the
generated `agent.yaml` indentation checked with `cat -A`.

**Safety constraints I am imposing (this writes into arbitrary user repos):**
- Never overwrite an existing file — per-file `create_new`, so user customizations survive.
- Never fail the indexing command if scaffolding fails (log and continue).
- Skip entirely when `.myrepograph-agent/` already exists (the documented trigger).
- Provide `REPOGRAPH_NO_SCAFFOLD=1` opt-out — creating ~20 files in someone's repo the first
  time they open it is a side effect they must be able to decline.

# Context Task Checklist - Scaffolding Status UI Integration

- [x] `main.rs`: `check_agent_scaffold` (read-only, never errors) + `trigger_agent_scaffold`
      (canonicalizes, rejects non-directories, reports failure); both in `generate_handler![]`
      (14 -> 16 commands).
- [x] `LeftSidebar.tsx`: `<AgentScaffoldStatus>` with `hasScaffold: boolean | null`, re-checked
      on every `activeProjectRoot` change; green pill / amber click-to-setup / inline error.
- [x] Verify: `cargo test` **88 passed / 0 failed** (74 + 11 + 2 + 1); `npm run build` clean;
      both UI states + click path + failure path exercised in the browser pane.

**Two defects found by verifying rather than assuming:**
1. **Test flake, fixed:** cargo runs a target's tests in parallel threads of ONE process, so the
   `REPOGRAPH_NO_SCAFFOLD` set by `opt_out_env_suppresses_scaffolding` was visible to the new
   command tests and could suppress scaffolding underneath them. Added `agent_scaffold::ENV_LOCK`
   and took it in every env-touching test.
2. **Layout, fixed:** the footer's `space-y-3` puts `margin-bottom` on `:not(:last-child)` only,
   and the button's computed `margin-bottom` was 0 once my card became the last child — the card
   rendered flush (gap 0px) while meter->button was 12px. Added explicit `mt-3`; both gaps are
   now 12px, measured.

**Not verified:** the real Rust commands over live IPC. The browser pane has no Tauri host, so the
UI was driven against a stubbed `window.__TAURI__` implementing the same contract; the Rust side is
covered by 4 unit tests instead. The native-window path is still a manual check.

**Path-safety note:** both new commands take a caller-supplied `project_root` string and write to
disk. `trigger_agent_scaffold` must canonicalize and reject anything that is not an existing
directory, so a malformed/hostile path cannot scatter template files somewhere unintended.

# Context Task Checklist - CEPA Interactive Guide Modal

- [x] `src/components/CEPAUserGuideModal.tsx` — 672px glass panel (max-w-2xl / rounded-2xl 16px /
      p-6 24px / bg-[#0F1218]/95 / backdrop-blur 24px / z-60), Orient-Target-Explore steps,
      the `signature_only` payoff card (1,473 chars, 98.56%), 2 collapsible copy cards.
- [x] `App.tsx` — mounted; auto-opens once per project via an `offeredFor` ref; toolbar re-open
      over a `repograph:open-cepa-guide` window event so the toolbar owns no modal state.
- [x] `TopToolbar.tsx` — `<HelpCircle size={15} />` "CEPA Guide" pill before the status badge.
- [x] Verify: `npm run build` clean, `cargo test` 88 passed; full flow exercised in-browser.

**Trigger bug in the brief, fixed:** it said auto-open on `status === 'updated'`, but
`openProject`/`selectProject` settle on `'synced'` — `'updated'` is only set by the
`graph_updated` watcher event. As written the guide would never appear on a fresh project open,
only after the user later edited a file. Now fires on `'synced' || 'updated'`, once per project.

**Verified in-browser (DOM + computed styles):** auto-open on index completion; "Copied!" tick in
emerald-300/emerald-500-15 reverting after 1.6s; each card copying its own body; dismissal writing
`repograph_cepa_dismissed=true`, suppressing the next project's auto-open, and being revocable by
un-ticking; toolbar re-open; Esc and backdrop close; inner clicks not closing.

**Prompt text proven identical to the docs by hash** — minimal 824 chars `fa1f2ef611c54940`,
architecture 398 chars `2aad8bb453f34fec`, matching `docs/AGENT_PROMPT_ARCHITECTURE.md` §3.1/§3.2.

**Dev-CSS red herring:** the emerald "Copied!" styling first measured as white/60. The utilities
were absent from the *dev server's* generated stylesheet for the newly created file; they are
present in the production bundle, and a hard reload made dev match. Not a code defect — but it is
why a computed-style check right after creating a file can lie.

**Spec bug to resolve:** the brief keys auto-open on `status === 'updated'`, but
`openProject`/`selectProject` set `'synced'` — `'updated'` is only set by the `graph_updated`
watcher event. Keying on `'updated'` means the guide NEVER appears on a fresh project open, only
after a later file edit. Trigger on indexing having finished (`synced` OR `updated`) instead.

**Prompt text must come verbatim from `docs/AGENT_PROMPT_ARCHITECTURE.md` §3.1/§3.2** — two copies
      of the same prompt that can drift is exactly the failure this guide is meant to prevent.

# Context Task Checklist - Final Production Release, Cleanup, & Sign-off

- [x] Compile optimized release target binary: `cd src-tauri && cargo build --release`
- [x] Execute full test suite: `cd src-tauri && cargo test --all-targets` (confirm 88/88 passing)
- [x] Compile production frontend assets: `npm run build` (confirm clean transpilation)
- [x] Clean up temporary assets: Remove any temporary scratch files or test data directories left in the workspace root
- [x] Write Walkthrough & Release Notes to `walkthrough.md`
- [x] Update `my-agent/memory/runtime/dailylog.md` to declare the release complete

**Close-out:** Compiled the Rust backend in release mode (`mcp_server.exe`), verified all 88 unit/integration tests passed, compiled the React frontend cleanly for production, removed the temporary `scratch` directory, and wrote the final walkthrough and daily log entries.

# Context Task Checklist - Windows Tauri Installer Build & Global MCP Registration

- [x] Compile the production installer: `npm run tauri build`
- [x] Verify creation of the `.msi` setup file and verify its size and presence
- [x] Provide global configuration updates for Claude Desktop and Codex Desktop configuration
- [x] Update `my-agent/memory/runtime/dailylog.md` to declare deployment complete

**Close-out:** Compiled the production installer bundle using `npm run tauri build` after adding `icon.ico` configuration in `tauri.conf.json`, generating `mcp_server_0.1.0_x64_en-US.msi`. Configured the global Claude Desktop and Codex configuration files to point to the newly compiled release binary `C:\My-pro\project-map\src-tauri\target\release\mcp_server.exe` with all tools enabled.

# Context Task Checklist - 10k-File Scale Optimizations (DB + Canvas)

- [x] `db.rs::populate_db`: `symbol_map` -> `HashMap<String, HashMap<String, i64>>` + shared `lookup_symbol()`; all 13 tuple-clone probe sites converted to borrowed-key lookups.
- [x] `db.rs::populate_db`: skip `std::fs::read_to_string` when `pf.extraction.symbols` is empty.
- [x] `store.ts`: `AUTO_COLLAPSE_NODE_LIMIT = 1000` + `autoCollapsedDirs()`; applied in `openProject`/`selectProject`, and in `load` only when `collapsedDirs` is empty (preserves user expansions on `graph_updated` reloads).
- [x] Verify: `cargo test --all-targets` 88/88; `npm run build` clean; `npm run tauri build` produced both MSI and NSIS bundles.
- [x] Record benchmarks in dailylog.md (structural analysis; honest note that a real 10k-repo wall-clock A/B was not run).

**Close-out:** Auto-collapse verified live in the browser pane: a synthetic 1,200-node graph fed through `load()` yielded `collapsedDirs.size = 160` (40 top-level + 120 subdirs, all ancestors present), while the real 131-node repo correctly stays at 0. Trap re-hit: a stale `target/debug/mcp_server.exe` (PID 29936) held a link lock and failed the first `cargo test`; killed and re-ran.

# Context Task Checklist - Real-Time Indexing Progress HUD

- [x] `indexer.rs`: `IndexProgress` payload struct + `emit_progress()` (no-op on `None` handle);
      `index_repo` emits `walking` -> `parsing` -> `db_write` -> `complete`.
- [x] `indexer.rs::parse_all`: `AtomicUsize`/`AtomicU64` counters updated by worker threads
      (`fetch_add` only, no IPC from workers); a monitor thread inside the same `thread::scope`
      polls every 100ms and emits `parsing` progress, breaking once `processed >= total`.
- [x] `index_repo(root, app_handle: Option<&tauri::AppHandle>)` — **deviation from the brief**
      (`&tauri::AppHandle` non-optional): `AppHandle` only exists once `tauri::Builder` has started
      an app, so the CLI (`mcp_server.rs`), `db::reconcile_repo_startup`, `build_graph_for`, and both
      integration tests have no handle to pass and would not compile against a required reference.
      All 5 non-Tauri call sites now pass `None`; `main.rs::index_and_load_graph` passes `Some(&app_handle)`.
- [x] `store.ts`: `IndexProgress`/`IndexPhase` types, `indexProgress` state, `setIndexProgress()` —
      EMA (`alpha=0.3`) over the instantaneous files/sec delta since the previous event, not a
      cumulative average, so a mid-run slowdown shows up within a poll or two. Reset via
      `resetIndexProgressTracking()` + `IDLE_INDEX_PROGRESS` at the start of `openProject`/`selectProject`.
- [x] `App.tsx`: `index_progress` Tauri event listener alongside the existing `graph_updated` /
      `agent_query_event` listeners, forwarding straight to `setIndexProgress`.
- [x] `ProjectHubDashboard.tsx`: glassmorphism HUD replacing the old bare spinner — progress bar,
      phase label, `files — bytes` status line, ETA line (gated to the `parsing` phase only, since
      `walking`/`db_write` are near-instant single-shot phases with no useful ETA).
- [x] Verify: `cargo test --all-targets` 88/88; `npm run build` clean; live-simulated via
      `window.__REPO_GRAPH_STORE__.getState().setIndexProgress(...)` in the browser pane (no
      Tauri host in-browser, so the IPC event itself can't be exercised, only the store+UI contract).

**Close-out:** Verified in-browser at 53% parsing (`6,500 / 12,300 files — 24.5 MB / 56.2 MB`,
`Remaining: ~1m 52s (~52 files/sec)`, computed EMA speed 51.7 files/sec matching a hand-fed
6,500-file delta), 100% `db_write` (no ETA line, correct — gated to `parsing`), and the
`isIndexing: false` path correctly falling back to the original hub CTA. **Not verified:** the real
Rust `emit_all` -> JS `listen` round trip, since the browser pane has no Tauri runtime (same
limitation noted in the 2026-07-22 Scaffolding Status UI session) — the store/UI logic is proven,
the IPC wiring itself is unit-tested only indirectly (compiles, existing `emit_all` pattern reused
verbatim from `agent_query_event`).

# Context Task Checklist - Global MCP Executable Path Fix

**Retrieved-facts correction:** the brief named `CEPAUserGuideModal.tsx` as the component with the
broken snippet. It has no MCP config snippet at all (verified: full-repo grep for
`mcpServers`/`mcp_server.exe`/`claude_desktop_config` inside `src/` matched only
`LeftSidebar.tsx`'s "Integrate Agent" modal). That is the actual component with the bug —
`mcpBinaryPath(root)` built `${root}/src-tauri/target/debug/mcp_server.exe`, i.e. assumed the
*opened project* contains Repo Graph's own source tree. Fixed that one; left
`CEPAUserGuideModal.tsx` untouched (nothing there references the binary path).
Also: **Retrieved Fact #3** (`agent_scaffold.rs` needs updating) does not apply — grepped it for
`mcp_server`/`src-tauri`/`target/debug`/`target/release`, zero matches; it writes no MCP path
config into `.myrepograph-agent/` at all.

- [x] `main.rs`: `get_mcp_config_snippet(project_root, app_handle)` + `resolve_mcp_binary()` —
      resolves the binary relative to **this running process** (`current_exe()`'s directory, then
      Tauri's `resource_dir()`), never relative to the caller-supplied `project_root`. Returns
      `binary_path` + `binary_exists` plus pre-rendered `claude_desktop_json` / `codex_toml` /
      `vscode_json` (VS Code's real schema: `{"servers": {"<name>": {"type": "stdio", ...}}}`,
      not `mcpServers`). Registered in `generate_handler!`.
- [x] `tauri.conf.json`: `bundle.resources: ["target/release/mcp_server.exe"]` (Rule 3) — makes
      `cargo tauri build` copy the binary into the installer.
- [x] `LeftSidebar.tsx`: integration modal now calls the IPC command on open (`useEffect` keyed on
      `showIntegrationModal`/`activeProjectRoot`), shows a loading state, an error state (browser
      dev build has no Tauri IPC), a ✓/⚠️ detection banner with a build hint when missing, and all
      3 host snippets + the CLI line generated from real backend output. Deleted the dead
      `mcpBinaryPath`/`mcpRootArg`/`mcpBinaryName`/`ALL_MCP_TOOLS` client-side guesses.
- [x] `.codex/config.example.toml`: rewrote the header to separate the two paths that were being
      conflated (`command` = wherever Repo Graph itself lives, fixed per machine; `args` = the
      target project) and points at the new in-app generator as the preferred source.
- [x] Verify: `cargo test --all-targets` 88/88; `npm run build` clean; `npm run tauri build` —
      inspected the **generated** `main.wxs` (MSI) and `installer.nsi` (NSIS) directly rather than
      performing a real system install: both place `mcp_server.exe` in the install root beside
      `Repo Graph.exe` (WiX: same `DirectoryRef Id="INSTALLDIR"` component as the `Path` component
      that holds the main exe; NSIS: `File /a "/oname=mcp_server.exe" ...` at `$INSTDIR` root) —
      confirms priority-1 sibling-of-`current_exe()` resolution finds it in the real installed app,
      not just in a dev checkout where both bins already happen to share `target/release/`.
- [x] Live-verified in the browser pane via a stubbed `window.__TAURI__.invoke('get_mcp_config_snippet', ...)`
      returning realistic Rust-shaped output: confirmed the modal renders the machine path
      (`C:/Program Files/Repo Graph/mcp_server.exe`) with the *target* project's root
      (`C:/Users/test/mifos_backend`) correctly separated in `args` — the exact bug fixed — plus
      the `⚠️ Binary Missing` state with its build hint.

**Not verified:** the real Rust `get_mcp_config_snippet` → JS `invoke` round trip and an actual MSI
install onto this machine (would leave installed state behind for a repo that isn't meant to be
installed here); backend correctness rests on `cargo build` + reading the generated installer
scripts, not a live install.

# Context Task Checklist - Walker Skip Filter Optimization

- [x] Exclude agent workspace directories in `src-tauri/src/walker.rs`: add `".myrepograph-agent"`, `"my-agent"`, and `".agents"` to `SKIP_DIRS`
- [x] Run backend tests: `cargo test`
- [x] Compile the production installer: `npm run tauri build`
- [x] Verify fix by editing files in `my-agent/` and checking that it does not trigger indexing
- [x] Update `my-agent/memory/runtime/dailylog.md` to document the fix

**Close-out:** Added `".myrepograph-agent"`, `"my-agent"`, and `".agents"` to `walker::SKIP_DIRS` in `src-tauri/src/walker.rs`. Verified that all 88 cargo unit and integration tests pass successfully and that modifying files inside the memory workspace directories no longer triggers any filesystem watch events or start-up catch-up indexing loops. Rebuilt the production bundles cleanly.





# Repo Graph — Principal Systems Audit

**Date:** 2026-07-26 · **Scope:** 5 vectors (Frontend, Rust engine, Parsers, MCP, Tests/Benchmarks)
**Method:** direct file inspection + the project's own test suite + two temporary empirical probes (since deleted). No project code was executed by any parser.

---

# Executive Audit Summary

**Overall Health Score: 62 / 100**

The engineering here is real and often above average: 88 Rust tests all pass, the parallel walker and the Zustand store are genuinely well-designed, and `docs/BENCHMARK_REPORT.md` is unusually honest (it documents a case where the tool *loses*). The score is dragged down by a small number of defects that are severe rather than numerous — one confirmed silent data-loss bug, one secrets-exposure path, and four hardcoded absolute paths that make the product unusable for anyone but the author.

**Verified baseline:** `cargo test --lib --tests` → **88 passed / 0 failed**.

### Top 3 Structural Risks

1. **Incremental re-index silently deletes the call graph it is supposed to maintain** (GAP-01). Empirically proven: 2 inbound edges → 0 after one file save. The watcher runs this path on *every* keystroke-debounced save, so the graph degrades continuously the longer the app is open. The agent-facing answer to "who calls this?" quietly becomes "nobody."
2. **The product is hardcoded to one developer's machine** (GAP-02). Four literal `C:\Users\nmahe\.gemini\antigravity\...` paths across `main.rs` and `mcp_server.rs`. Non-portable to any other user and to every non-Windows platform. Worse, that file *is* the MCP server's confinement root — an external, world-writable JSON file decides which directory the agent may read.
3. **`.env` and every other secret-bearing dotfile is indexed and served to agents** (GAP-03). Empirically proven: `read_file(".env")` returned a live `DATABASE_URL` with credentials.

---

# 1. Architectural Gap Matrix

| ID | Subsystem | Gap / Vulnerability | File / Line | Severity | Impact |
|:---|:---|:---|:---|:---|:---|
| GAP-01 | Engine / Persistence | Incremental re-index of a file destroys all **inbound** cross-file symbol and file edges and never rebuilds them | `src-tauri/src/db.rs#L1127`, schema `#L41,53-54,66-67` | **CRITICAL** | Proven 2→0. `repograph_callers`/`impact` return false negatives after any save |
| GAP-02 | Portability / MCP root | Four hardcoded `C:\Users\nmahe\.gemini\antigravity\` paths; MCP confinement root read from that external file | `mcp_server.rs#L46`, `main.rs#L34,89,99` | **CRITICAL** | Unusable for any other user or OS; external file controls the read sandbox |
| GAP-03 | Walker / MCP | `hidden(false)` with no `.env` denylist → secrets indexed into SQLite and served by `read_file` | `walker.rs#L101`, `SKIP_FILES#L37` | **CRITICAL** | Proven: credentials returned verbatim to the agent |
| GAP-04 | Manifest / Graph | `build_manifest` calls `dependencies_of()` per node; that is a full linear scan of `edges` | `manifest.rs#L57`, `graph.rs#L412` | **CRITICAL** | O(N·E). 10k nodes × 50k edges ≈ 5×10⁸ string compares per `repograph_files` call |
| GAP-05 | MCP staleness | `check_staleness`/`repograph_status` read a process-local static the MCP process never populates | `mcp_server.rs#L689,744`, `watcher.rs#L8` | **MAJOR** | MCP server always reports "Synced". The entire staleness safety net is dead where it matters |
| GAP-06 | Parsers | Only JS/TS is AST-based. `c_cpp/swift/html/sql/markdown/dockerfile` emit **zero** symbols; `java/csharp/php/kotlin` emit them only for route handlers and ORM entities | `parsers/java.rs`, `swift.rs`, `c_cpp.rs`, `csharp.rs` | **MAJOR** | `repograph_explore` — the only tool exposed by default — is blind to most non-JS code |
| GAP-07 | Parsers / JS | A fully-parsed module is discarded whenever `take_errors()` is non-empty; `tsx:true` is forced on `.ts` files | `javascript.rs#L418-429` | **MAJOR** | `.ts` generics (`<T,>(x)=>x`) and old-style casts parse as JSX → spurious `parse_error`, whole file unmapped |
| GAP-08 | Engine / Startup | `reconcile_repo_startup` computes a precise dirty-file list, discards it, and runs a **full** `index_repo` | `db.rs#L995-1005` | **MAJOR** | Called from `McpServer::new` — MCP handshake blocks on a full repo re-index |
| GAP-09 | MCP protocol | `REPOGRAPH_MCP_TOOLS` filters `tools/list` only; `tools/call` dispatches every tool unconditionally | `mcp_server.rs#L295-352` vs `#L1024-1192` | **MAJOR** | Not an authorization boundary. Any client can call hidden tools by name |
| GAP-10 | Persistence perf | `populate_db` EventEmitter solver is an O(emitters × receivers) all-pairs loop, re-reading every file from disk | `db.rs#L660-733` | **MAJOR** | Quadratic on JS repos; doubles index-time I/O |
| GAP-11 | Watcher | Deletes and renames are never handled (`Modify`/`Create` only); watcher + polling thread leak on every root change | `watcher.rs#L101,117-122` | **MAJOR** | Deleted files persist in the graph forever; old roots keep marking pending after project switch |
| GAP-12 | Security / IPC | `open_in_editor` shells through `cmd /c code <path>` on Windows | `main.rs#L430-431` | **MAJOR** | Filename containing `&`/`\|`/`^` inside an indexed repo → command injection on click |
| GAP-13 | Persistence perf | `log_agent_query` calls `init_db` (10 DDL statements + PRAGMA introspection) per call; `explore` calls it per symbol | `db.rs#L1447`, `mcp_server.rs#L560` | **MAJOR** | 5-symbol explore opens 6 connections and re-runs schema setup 6× |
| GAP-14 | Frontend / IPC | `read_graph` returns the entire graph — nodes, all symbols, all symbol_edges — as one IPC blob | `main.rs#L297-328`, `loadGraph.ts#L39` | **MAJOR** | Tauri v1 stringifies through the webview bridge; tens of MB at 10k nodes |
| GAP-15 | Frontend perf | `buildFlow` iterates **all** `symbol_edges` on every relayout, even with nothing expanded | `layout.ts#L151-182` | **MAJOR** | Every filter/collapse toggle re-walks the full symbol edge set |
| GAP-16 | Frontend state | `setTimeout` side effect fired from inside a Zustand `set()` updater; O(nodes×symbols) scan per activity | `store.ts#L462-483` | MINOR | Untracked timers, no cleanup, impure reducer |
| GAP-17 | Persistence | `agent_queries` grows unbounded; no retention or pruning | `db.rs#L92-101,1447` | MINOR | `graph.db` bloat over long sessions |
| GAP-18 | Graph | `exports.dedup()` without a prior sort only removes *consecutive* duplicates | `graph.rs#L178` | MINOR | Non-adjacent duplicate exports survive into the manifest |
| GAP-19 | Walker | `SKIP_DIRS` matches by name at any depth and hardcodes project-specific `my-agent`, `.agents`; `.gitignore` is inert outside a git repo (`require_git` default) | `walker.rs#L19-35,100-103` | MINOR | A legitimate `src/build/` source dir is skipped; gitignore silently unenforced here |
| GAP-20 | Parsers | Line scanners skip a block comment only when the line *starts* with `//`, `/*`, `*`; `contains("class ")` matches inside string literals | `java.rs#L13,41`, `csharp.rs`, `kotlin.rs`, `swift.rs` | MINOR | False exports from commented-out and quoted code |
| GAP-21 | Tests | Zero frontend tests; no CI; env-var mutation inside a multithreaded test | none / `mcp_server.rs#L1509-1533` | MINOR | 2,900 lines of untested TS; `tools_list` test can race |
| GAP-22 | Docs | `docs/logic/token-optimization-logic.md` still documents `schema_version: 1` and `read_file(path)`; both renamed | `token-optimization-logic.md#L12,48` | MINOR | Spec drift from `MANIFEST_SCHEMA_VERSION = "1.1.0"` / `repograph_node` |

---

# 2. Detailed Findings & Remediation

## GAP-01 — Incremental re-index destroys inbound edges (CRITICAL)

**Root cause.** `populate_file_in_db` begins with `DELETE FROM files WHERE path = ?` (`db.rs:1127`). The schema declares `symbols.file_path REFERENCES files(path) ON DELETE CASCADE` (`db.rs:41`) and `edges.from/to_symbol_id REFERENCES symbols(id) ON DELETE CASCADE` (`db.rs:53-54`), with `PRAGMA foreign_keys=ON` (`db.rs:9`). So deleting the file row cascades away *every* edge touching that file's symbols — including edges whose **source** is a different, untouched file. Re-insertion mints fresh `AUTOINCREMENT` ids, and the only code that could recreate an `a.ts → b.ts` edge is a re-parse of `a.ts`, which never happens. `file_edges` has the identical problem via its `to_path` FK (`db.rs:66-67`).

**Empirical proof.** `a.ts` imports and calls `b.ts#target`. Full index → 2 inbound edges. One `populate_file_in_db` on `b.ts` → **0**.

The existing regression test `repeated_incremental_reindex_does_not_grow_symbols_fts` (`db.rs:1593`) uses a **single** file, so it cannot observe this.

**Proposed solution.** Re-index the file's *dependents* alongside it. `file_edges` already stores the reverse direction, so the set is one query away.

```diff
--- a/src-tauri/src/watcher.rs
+++ b/src-tauri/src/watcher.rs
@@ fn reparse_file(root: &Path, file_path: &Path) -> Result<(), String> {
+    // Re-indexing a file cascades away every edge pointing INTO it, including
+    // those owned by untouched dependents. Collect them before the delete and
+    // re-parse them too, or the call graph erodes on every save.
+    let dependents: Vec<String> = {
+        let mut stmt = conn
+            .prepare("SELECT DISTINCT from_path FROM file_edges WHERE to_path = ?")
+            .map_err(|e| e.to_string())?;
+        let rows = stmt
+            .query_map([&rel_path], |r| r.get::<_, String>(0))
+            .map_err(|e| e.to_string())?;
+        rows.filter_map(|r| r.ok()).filter(|p| p != &rel_path).collect()
+    };
+
     crate::db::populate_file_in_db(&conn, &parsed, root, &aliases, &file_set)
         .map_err(|e| e.to_string())?;
+
+    for dep in dependents {
+        let dep_abs = root.join(&dep);
+        let Ok(src) = std::fs::read_to_string(&dep_abs) else { continue };
+        let dep_entry = crate::walker::FileEntry {
+            path: dep.clone(),
+            size_bytes: src.len() as u64,
+            extension: dep_abs.extension().and_then(|s| s.to_str()).unwrap_or("").to_string(),
+            language: crate::walker::language_for(&dep_abs),
+        };
+        let dep_parsed = crate::indexer::parse_one(root, dep_entry);
+        let _ = crate::db::populate_file_in_db(&conn, &dep_parsed, root, &aliases, &file_set);
+    }
     Ok(())
 }
```

**Required test** (currently absent): index two files where A calls B, re-index B alone, assert the inbound edge count is unchanged.

---

## GAP-02 — Hardcoded machine paths, and an external file controls the MCP sandbox (CRITICAL)

**Root cause.** Four occurrences:

| File | Line | Path |
|:--|--:|:--|
| `mcp_server.rs` | 46 | `active_project.json` — **read to decide the confinement root** |
| `main.rs` | 34 | `active_project.json` — written on every project open |
| `main.rs` | 89 | `recent_projects.json` — read |
| `main.rs` | 99 | `recent_projects.json` — written |

Two distinct defects share one line of code. The portability defect is obvious. The security defect is not: `resolve_mcp_root` (`mcp_server.rs:42-74`) reads `active_project_root` out of that JSON and canonicalizes it into `self.root` — the single boundary every `resolve_inside_root` check is measured against. Anything that can write that file can repoint the agent's readable universe at `C:\Users\<me>\` and every subsequent `read_file` will pass its own confinement check.

The confinement *logic* itself is correct — `canonicalize()` then component-wise `Path::starts_with` (`mcp_server.rs:988-996`), which resists `..`, absolute paths, and symlink escapes. `read_file_rejects_path_traversal` passes. The boundary is sound; its *origin* is not.

**Proposed solution.** Derive the state directory from the platform, and treat the registry as an untrusted hint that must be re-validated against a root the user explicitly chose.

```diff
+/// Per-user state dir. Never a literal home path — that is not portable and
+/// makes the registry (which decides the MCP confinement root) world-writable
+/// at a predictable location.
+fn state_dir() -> Option<std::path::PathBuf> {
+    #[cfg(target_os = "windows")]
+    let base = std::env::var_os("APPDATA").map(std::path::PathBuf::from);
+    #[cfg(not(target_os = "windows"))]
+    let base = std::env::var_os("XDG_STATE_HOME")
+        .map(std::path::PathBuf::from)
+        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".local/state")));
+    base.map(|b| b.join("repo-graph"))
+}
+
+fn active_project_registry() -> Option<std::path::PathBuf> {
+    state_dir().map(|d| d.join("active_project.json"))
+}
```

In `resolve_mcp_root`, prefer an explicit argument or `REPO_GRAPH_ROOT`; fall back to the registry **only** when it names a directory that already contains `.repograph/graph.db`, and surface which root was chosen in the `initialize` response so the operator can see it.

---

## GAP-03 — `.env` indexed and served (CRITICAL)

**Root cause.** `walker.rs:101` sets `.hidden(false)` — the comment justifies it as indexing `.env.example`, but nothing distinguishes the example from the real file. `SKIP_FILES` (`walker.rs:37-47`) lists only lockfiles. `.env` resolves to language `other`, so it is never *parsed* — but it is still a graph node, still listed in the manifest, and still fully readable through `read_file`.

**Empirical proof.** Walker returned `[".env", "app.ts"]`; `read_file(".env")` returned `Ok("DATABASE_URL=postgres://u:secret@h/db\n")`.

**Proposed solution.** Deny secrets at the walker *and* enforce it again at the MCP read boundary — an index built before the fix must not keep leaking.

```diff
+/// Denied at the walker AND re-checked in `read_file`: a cache built before
+/// this list existed still holds these paths as nodes.
+pub fn is_secret_file(name: &str) -> bool {
+    if name == ".env" || name.starts_with(".env.") {
+        // `.env.example` / `.env.sample` / `.env.template` are meant to ship.
+        return !matches!(
+            name.rsplit('.').next().unwrap_or(""),
+            "example" | "sample" | "template" | "dist"
+        );
+    }
+    matches!(
+        name,
+        ".npmrc" | ".pypirc" | ".netrc" | "credentials" | ".pgpass"
+            | "id_rsa" | "id_ed25519" | ".htpasswd"
+    ) || name.ends_with(".pem") || name.ends_with(".key") || name.ends_with(".p12")
+}
```

```diff
--- a/src-tauri/src/mcp_server.rs
+++ b/src-tauri/src/mcp_server.rs
     pub fn read_file(&self, path: &str) -> Result<String, ToolError> {
         let resolved = self.resolve_inside_root(path)?;
+        let name = resolved.file_name().and_then(|n| n.to_str()).unwrap_or("");
+        if crate::walker::is_secret_file(name) {
+            return Err(ToolError::new(
+                "secret_file_blocked",
+                format!("'{path}' looks like a secrets file and is never served"),
+            ));
+        }
```

---

## GAP-04 — Manifest generation is O(N·E) (CRITICAL for the stated 10k-node target)

**Root cause.** `Graph::dependencies_of` / `dependents_of` / `node` are linear scans over `self.edges` (`graph.rs:406-431`). `build_manifest` calls `dependencies_of` once per node (`manifest.rs:57`), so `repograph_files` is O(N·E). `repograph_impact` in file mode calls `dependents_of` inside a BFS (`mcp_server.rs:954`) — O(V·E).

The irony worth naming: `src/store.ts:73` carries the comment *"O(1) adjacency lookups (RULES.md §4: no linear scans per query)"* and builds exactly the right `Map` indexes in `ingestGraph`. The frontend obeyed the rule; the Rust side, which serves the agents, did not.

**Proposed solution.** Build the same adjacency index once, lazily, on `Graph`.

```diff
+#[derive(Debug, Default)]
+pub struct Adjacency {
+    pub deps: HashMap<String, Vec<String>>,
+    pub dependents: HashMap<String, Vec<String>>,
+    pub by_path: HashMap<String, usize>,
+}
+
 impl Graph {
+    /// Built once per load. Mirrors `ingestGraph` in `src/store.ts` — the
+    /// frontend has had O(1) adjacency since day one; the agent API has not.
+    pub fn adjacency(&self) -> Adjacency {
+        let mut a = Adjacency::default();
+        for (i, n) in self.nodes.iter().enumerate() {
+            a.by_path.insert(n.path.clone(), i);
+        }
+        for e in &self.edges {
+            if e.kind == EdgeKind::Route { continue; }
+            a.deps.entry(e.from_path.clone()).or_default().push(e.to_path.clone());
+            a.dependents.entry(e.to_path.clone()).or_default().push(e.from_path.clone());
+        }
+        for v in a.deps.values_mut().chain(a.dependents.values_mut()) {
+            v.sort();
+            v.dedup();
+        }
+        a
+    }
 }
```

Cache it beside `graph` in `McpServer` and invalidate on the same mtime check that already exists at `mcp_server.rs:1002-1017`. Expected: `repograph_files` on a 10k-file repo drops from ~10⁸ comparisons to ~10⁵.

---

## GAP-05 — Staleness detection is inert in the MCP process (MAJOR)

**Root cause.** `PENDING_CHANGES` is a `static Mutex<Option<HashMap<..>>>` in `watcher.rs:8` — **process-local**. It is filled only by `start_watcher`, which requires a `tauri::AppHandle` and is called only from the Tauri commands `read_graph` and `index_and_load_graph` (`main.rs:303,358`). The `mcp_server` binary (`src/bin/mcp_server.rs`) never has an `AppHandle` and never starts a watcher.

Consequence: in the MCP server — the process agents actually talk to — `get_pending_map()` always returns empty. `check_staleness` therefore never emits its "⚠️ edited since last sync" header, and `repograph_status` unconditionally reports **"Sync State: Synced / All files are fully indexed."** That is an affirmative false assurance, which is worse than no signal.

**Proposed solution.** Move pending state to shared storage both processes can see — a `pending_changes(path TEXT PRIMARY KEY, marked_at INTEGER)` table in the existing `graph.db` (WAL is already enabled, so cross-process reads are cheap). `mark_pending`/`remove_pending`/`get_pending_map` become thin queries. Until that lands, `codegraph_status` should say *"watcher not attached to this process — freshness unknown"* rather than "Synced".

---

## GAP-06 / GAP-07 — Parser fidelity (MAJOR)

**GAP-06 root cause.** Symbol emission by extractor, counted directly:

| Emits real symbols | Route/ORM symbols only | **Zero symbols** |
|:--|:--|:--|
| `javascript` (11 sites, swc AST), `python`, `rust_lang`, `go`, `sfc` (delegates to JS) | `java`, `csharp`, `php`, `kotlin` | `c_cpp`, `swift`, `html`, `sql`, `markdown`, `dockerfile` |

`java.rs` is representative: it pushes class names to `exports` (line 50) but never to `symbols`, and extracts no methods at all. Since `repograph_explore` resolves against the `symbols` table, and it is the **only tool exposed by default** (`mcp_server.rs:1034`), an agent working in a C++, Swift, or ordinary Java codebase gets nothing back.

**Recommendation.** Two of these are cheap to close and should come first: `java.rs` and `csharp.rs` already locate class/interface declarations — pushing an `ExtractedSymbol` at the same site is a few lines each and immediately makes `explore` useful for JVM/.NET repos. Method-level extraction and C/C++/Swift need either brace-depth tracking or `tree-sitter` grammars; scope that separately.

**GAP-07 root cause.** `javascript.rs:412-429`:

```rust
let syntax = Syntax::Typescript(TsSyntax { tsx: true, decorators: true, ..Default::default() });
...
if parser.take_errors().is_empty() { Ok((cm, module)) } else { Err(()) }
```

Two problems. `tsx: true` is applied unconditionally, so in a plain `.ts` file the parser reads `const f = <T,>(x: T) => x` and `<string>value` as JSX. And `take_errors()` returns *recoverable* diagnostics — a module that parsed fine is thrown away, the file is marked `parse_error`, and every symbol, import, and edge in it is lost.

```diff
-    let syntax = Syntax::Typescript(TsSyntax { tsx: true, decorators: true, ..Default::default() });
+    // `tsx` must follow the extension: with it forced on, a `.ts` file's
+    // generic arrows (`<T,>(x) => x`) and casts (`<string>v`) parse as JSX.
+    let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
+    let syntax = Syntax::Typescript(TsSyntax {
+        tsx: matches!(ext, "tsx" | "jsx"),
+        decorators: true,
+        ..Default::default()
+    });
     let mut parser = Parser::new(syntax, StringInput::from(&*fm), None);
     let module = parser.parse_module().map_err(|_| ())?;
-    if parser.take_errors().is_empty() { Ok((cm, module)) } else { Err(()) }
+    // Recoverable errors are not fatal: swc still produced a usable module.
+    // Discarding it loses every symbol and edge in an otherwise fine file.
+    Ok((cm, module))
```

To keep the honesty the manifest depends on, have `extract` push a `partial_parse` warning when `take_errors()` was non-empty, instead of dropping the file.

---

## GAP-09 — Tool allowlist is cosmetic (MAJOR)

`tools_list_result()` gates each tool on `REPOGRAPH_MCP_TOOLS` (`mcp_server.rs:1024-1192`), defaulting to `explore` alone. But `tools_call` (`mcp_server.rs:295-352`) matches on the tool name with no reference to that list. A client that already knows the names — they are in this repo's docs and in `main.rs`'s generated config snippet — calls any of them regardless.

If the intent is purely UX (keep the agent's tool menu small), that is fine and the fix is a comment. If it is meant as a capability boundary, apply the same list in `tools_call`:

```diff
     fn tools_call(&mut self, params: &Value) -> Result<Value, (i64, String)> {
         let name = params.get("name").and_then(Value::as_str)
             .ok_or((-32602, "missing tool name".to_string()))?;
+        // The allowlist has to gate dispatch too — filtering `tools/list`
+        // alone only hides names a caller can guess from the docs.
+        if !tool_is_enabled(name) {
+            return Err((-32601, format!("tool not enabled: {name}")));
+        }
```

---

## GAP-12 — Editor launch shells through `cmd /c` (MAJOR)

`main.rs:430-431` builds `cmd /c code <path>`. The path is canonicalized and root-confined, so traversal is blocked — but confinement does not help here. `cmd.exe` re-parses its command line with its own metacharacter rules, and Rust's argument quoting targets the standard C runtime, not `cmd`. A file named `note&calc.txt` inside an indexed repository becomes an execution primitive the moment the user clicks it.

```diff
     #[cfg(target_os = "windows")]
     {
-        let mut cmd = std::process::Command::new("cmd");
-        cmd.arg("/c").arg("code").arg(&clean_path);
+        // Never route through `cmd /c`: it re-parses the line with its own
+        // metacharacter rules, so a repo file named `a&calc.txt` executes.
+        let mut cmd = std::process::Command::new("code.cmd");
+        cmd.arg(&clean_path);
         let status = cmd.status().map_err(|e| format!("failed to spawn editor: {e}"))?;
```

(On Rust ≥1.77 `Command` refuses to run `.bat`/`.cmd` with un-escapable arguments rather than mis-escaping them, so this is the safe form. If `code.cmd` is not on `PATH`, resolve it explicitly instead of falling back to a shell.)

---

## GAP-08 / GAP-10 / GAP-13 — Indexing and query cost (MAJOR)

- **GAP-08.** `reconcile_repo_startup` (`db.rs:974-1005`) walks the repo, compares size+mtime per file, builds `files_to_reindex` — and then calls `index_repo(root)` on everything, discarding the list. Because `McpServer::new` calls it (`mcp_server.rs:214`), a single changed file blocks the MCP handshake on a full re-index. Fix: feed `files_to_reindex` to `populate_file_in_db` (with the GAP-01 dependent fix applied), and fall back to a full index only when the file set itself changed.
- **GAP-10.** The EventEmitter solver (`db.rs:660-733`) re-reads every file from disk — already read during parsing — then runs an all-pairs `emitters × receivers` loop. Fix: collect the channel strings during extraction, and match through a `HashMap<channel, Vec<receiver>>`.
- **GAP-13.** `log_agent_query` calls `init_db` (`db.rs:1448`), which executes ~10 `CREATE TABLE/INDEX` statements plus a `PRAGMA table_info` probe — on every logged query. `explore` calls it once per symbol *and* opens its own connection. Fix: pass the existing `&Connection` down; keep one connection on `McpServer`.

---

## Frontend notes (GAP-14 / GAP-15 / GAP-16)

Credit where it is due: `store.ts` and `GraphCanvas.tsx` are the best-engineered part of this repo. `ingestGraph` builds O(1) adjacency maps; `setHovered` swaps a CSSOM rule and mirrors into the store once per animation frame, so a mouse sweep over 1,000 nodes costs zero React renders; `autoCollapsedDirs` guards past 1,000 nodes; `onlyRenderVisibleElements` gives real viewport clipping; layout past 500 nodes is deferred behind an overlay using `setTimeout` rather than `rAF` (correct — `rAF` is throttled to zero in a hidden window).

The remaining gaps are at the seams:

- **GAP-14.** `read_graph` returns the whole graph, symbols and symbol-edges included, as one Tauri IPC payload. Serve the manifest first and fetch symbols per file on expand.
- **GAP-15.** `buildFlow` scans every `symbol_edge` on each relayout even when `expandedFiles` is empty (`layout.ts:151`). Guard with `if (expandedFiles.size > 0)` and index symbol edges by file.
- **GAP-16.** `addAgentActivity` schedules a `setTimeout` from inside a `set()` updater (`store.ts:462`). Move it to the caller or a subscription; track handles so they can be cleared.

**State resilience** (the Vector-1 question) is handled properly: `load`/`openProject`/`selectProject` all catch, set `status: 'stale'`, and surface `loadError`, which `GraphCanvas` renders as an actionable panel telling the user to run `mcp_server index`. Partial parses reach the UI as manifest warnings. This is the one subsystem with no gap worth filing.

---

# 3. Benchmark Verification & Token Savings Assessment

**Verdict: the numbers are credible and unusually well-caveated, but none of them are automated, and the token counts are estimated rather than tokenized.**

| Claim | Source | Status |
|:--|:--|:--|
| 98.56% savings, tier 5 | `BENCHMARK_REPORT.md` §2 | **Plausible, manually measured.** Arithmetic checks: 1 − 398/27,620 = 98.56% |
| Token counts | §3, `C / 3.7` | **Estimated, not measured.** A fixed chars-per-token divisor; code tokenizes worse than prose, so both arms are likely undercounted |
| `compact_edges` smaller than object form | `mcp_server.rs:1294-1300` | **Automated and passing** — the only benchmark property with a test |
| Manifest ≤2–3K tokens for a mid-size repo | `token-optimization-logic.md` §Token budget | **Unverified.** No test asserts it |
| "Fail the build if manifest exceeds budget by 20%" | same doc, §Validation | **Not implemented.** There is no CI at all (no `.github/`) |
| 664 MB → 0.37 MB cache fix | §Cache Growth Warning | **Verified.** Root cause correctly diagnosed; regression tests `build_output_is_never_re_indexed` and `real_sources_are_watched_including_newly_supported_languages` exist and pass |

Two things deserve explicit praise, because they are rarer than the defects above: §5 documents a measured case where `explore` costs **1.31× more** than a plain file read, and §2's preamble instructs the reader to *"quote the tier that matches the question."* That is the opposite of benchmark inflation.

Two caveats the report should carry:

1. **The 98.56% figure is measured on this repository's own 136 files.** The >90% target is stated for *large* repos, and the manifest path — which §5 correctly identifies as "where the >90% savings genuinely live" — is precisely the path that is O(N·E) (GAP-04). The headline number has never been measured at the scale it claims to serve.
2. **Savings are computed against a hand-picked Arm A** (7 files a human chose). A fairer baseline would be what an agent *actually* reads when solving the task without the tool.

### Recommendations

1. **Make the benchmark a test.** Fixture repo → assert manifest token budget and tier-5 char count within ±20%. This is already specified in `token-optimization-logic.md` §Validation and simply was not built.
2. **Count tokens with a real tokenizer** (`tiktoken-rs` or equivalent) instead of `C/3.7`, and label the current figures "estimated" until then.
3. **Benchmark at target scale.** Generate a synthetic 10k-file / 50k-edge repo and measure `repograph_files` latency. Fix GAP-04 first — the current implementation will not produce a usable number.
4. **Fix GAP-01 before quoting any call-graph savings.** The `paths` array is a large share of the tier-5 payload and *is* the answer to the reference query; after one file save in a live session it is silently incomplete.

---

# 4. Actionable Remediation Roadmap

### Phase 1 — Immediate (correctness, security, portability)

1. **GAP-01** — re-index dependents on incremental update; add the two-file regression test. *Nothing else matters if the graph decays while the app is running.*
2. **GAP-02** — replace all four hardcoded paths with a platform state dir; validate the registry root before trusting it as the confinement boundary.
3. **GAP-03** — `is_secret_file` denylist in the walker plus a re-check in `read_file`.
4. **GAP-12** — drop `cmd /c` from `open_in_editor`.
5. **GAP-05** — at minimum, stop reporting "Synced" from a process with no watcher.

### Phase 2 — Engine & parsers

6. **GAP-04** — `Graph::adjacency()`, cached on `McpServer`.
7. **GAP-08** — make startup reconciliation actually incremental.
8. **GAP-07** — extension-driven `tsx`; stop discarding recoverable-error modules.
9. **GAP-06** — emit class/interface symbols from `java.rs` and `csharp.rs` (cheap, high value); scope C/C++/Swift separately.
10. **GAP-09** — decide whether the tool allowlist is a boundary or a menu, then make the code say so.
11. **GAP-11** — handle `Remove`/`Rename` events; stop leaking watcher threads on root change.
12. **GAP-10 / GAP-13** — de-quadratic the EventEmitter solver; stop re-running DDL per query.

### Phase 3 — Frontend, testing, docs

13. **GAP-14 / GAP-15** — paginate the IPC payload; gate the symbol-edge scan on `expandedFiles`.
14. **GAP-21** — add Vitest and cover `ingestGraph`, `buildFlow`, `simulateImpact`, `setIndexProgress`; stand up CI running `cargo test` + `tsc -b` + `eslint`.
15. **Benchmark automation** — implement the §Validation budget assertion the docs already specify.
16. **GAP-16 / GAP-17 / GAP-18 / GAP-19 / GAP-20 / GAP-22** — the cleanup tail.

# Repo Graph — Audit Scratchpad

Date: 2026-07-26 · Mode: static inspection + project's own test suite. No project code executed by parsers.

## Baseline verified
- `cargo test --lib --tests` → **88 passed, 0 failed** (74 lib + 11 main + 2 integration + 1 route binding).
- 89 `#[test]` attrs in Rust. **0** frontend tests; no vitest/jest/testing-library in package.json. No `.github/` CI.

## Empirically proven (temporary probe, since deleted)
1. **Incremental re-index destroys inbound cross-file symbol edges.**
   Probe: a.ts imports+calls b.ts#target → full index yields 2 inbound edges →
   `populate_file_in_db` on b.ts alone → **0**. `DELETE FROM files WHERE path=?`
   cascades symbols→edges (db.rs:41,53-54); new symbol ids are minted; a.ts is
   never re-parsed. Watcher runs exactly this path on every save.
2. **`.env` is walked, indexed, and served verbatim by MCP `read_file`.**
   Probe: walker returned `[".env", "app.ts"]`; `read_file(".env")` returned
   `Ok("DATABASE_URL=postgres://u:secret@h/db\n")`. walker.rs:101 `hidden(false)`,
   no `.env` in SKIP_FILES.

## Corrections made during audit
- sfc.rs shows 0 `symbols.push` but **does** inherit symbols by delegating to
  JsExtractor and merging (sfc.rs:44-68). Not a gap.
- `languageFilters` default `['javascript','python','rust','other']` does NOT
  hide Go/Java/etc — `normalizeLanguage` (layout.ts:187) buckets all 16 other
  languages into `'other'`, which IS in the default set. The gap is
  indistinguishability in the UI, not invisibility.

## Vector notes
V1 Frontend: store.ts is the strongest layer — O(1) adjacency maps, rAF hover
   coalescing, auto-collapse >1000 nodes, `onlyRenderVisibleElements`.
   Gaps: whole-graph single IPC blob; symbol_edges rescanned every buildFlow;
   setTimeout side effect inside a zustand set() updater.
V2 Rust: walker/indexer parallelism is sound (scoped threads, atomics).
   Gaps: hardcoded `C:\Users\nmahe\...` ×4; Graph::* linear scans; reconcile
   does full reindex; watcher ignores deletes; watcher threads leak per root.
V3 Parsers: only JS/TS is AST (swc). 18 others are line scanners.
   c_cpp/swift/html/sql/markdown/dockerfile emit **zero** symbols;
   java/csharp/php/kotlin emit symbols only for route handlers + ORM entities.
   `parse()` discards a fully-parsed module if take_errors() is non-empty.
   `tsx:true` forced on `.ts` files.
V4 MCP: path confinement itself is correct (canonicalize + component-wise
   starts_with, traversal test passes). But the *root* comes from an external
   JSON file. REPOGRAPH_MCP_TOOLS filters tools/list only, not tools/call.
   check_staleness reads a process-local static the MCP process never fills.
V5 Tests/Benchmarks: benchmark numbers are real measurements, honestly caveated
   (incl. a counter-case where explore loses). But token counts are estimated
   at 3.7 chars/token, not tokenizer-derived, and none of it is automated.

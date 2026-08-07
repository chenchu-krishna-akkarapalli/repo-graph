# Logic: Token Optimization / Manifest Generation

Owns: turning the internal `Graph` into the compact, agent-facing manifest
that gets injected into a system prompt or returned from `get_manifest()`.
This is the whole point of the project — get this right and everything
upstream (walker, parsers, graph) is in service of it.

## Manifest schema (JSON, source of truth)

> **Schema version is a semver string, not an integer.** `manifest.schema_version`
> is `"1.1.0"` (`manifest::MANIFEST_SCHEMA_VERSION`) and is deliberately
> distinct from `graph.schema_version`, the integer identifying the on-disk
> `.repograph/graph.json` cache layout, which remains `1`.

```json
{
  "schema_version": "1.1.0",
  "generated_at": "2026-07-18T00:00:00Z",
  "root": "/path/to/repo",
  "files": [
    {
      "path": "src/components/Button.tsx",
      "exports": ["Button"],
      "depends_on": ["src/hooks/useTheme.ts"],
      "route": null
    },
    {
      "path": "src/pages/api/login.ts",
      "exports": [],
      "depends_on": ["src/lib/db.ts"],
      "route": "/api/login"
    }
  ],
  "external_dependencies": ["react", "lodash"],
  "warnings": [
    { "path": "src/x.ts", "kind": "unresolved_dynamic" }
  ]
}
```

## Markdown rendering (what actually goes in the agent's prompt)

Rendered deterministically from the JSON — never hand-edited, never a
separate source of truth:

```markdown
## Project Architecture Map (Active Root: /repo)
- /src/components/Button.tsx (Exports: Button) (Depends on: /src/hooks/useTheme.ts)
- /src/pages/api/login.ts (Route: /api/login) (Depends on: /src/lib/db.ts)

## Agent Instructions (payload format v1.1.0)
You are looking at a compressed map. Do NOT guess file contents.
Use `repograph_explore(symbols)` for symbol slices with call graphs, or `repograph_node(path)` to read a whole file, before editing it.
For architecture questions use `repograph_explore(symbols, signature_only: true)` — declarations plus the call graph, without the bodies.
```

The tools were renamed: `get_manifest` → `repograph_files`, `read_file` →
`repograph_node`. `manifest::tests::markdown_golden_render` is the byte-exact
source of truth for this block.

## Compression rules

- One line per file, fixed format: `path (Exports: ...) (Depends on: ...)`
  or `path (Route: ...) (Depends on: ...)` — omit empty fields entirely
  rather than printing `(Exports: none)`.
- Files with zero exports, zero routes, and zero meaningful dependents
  (e.g. config files, type-only files with no consumers) can be collapsed
  into a single summary line per directory instead of one line each, if the
  manifest exceeds a configurable line budget (default: 500 lines).
- Sort by directory depth then alphabetically, so the manifest reads like a
  tree even though it's flat text.
- Always include `warnings` at the end as a short bullet list — agents
  should know when the map has known gaps (e.g. dynamic imports it
  couldn't resolve) rather than silently trusting an incomplete map.

## Token budget targets

- Manifest should stay under ~2-3K tokens for a mid-size repo (a few
  hundred files) so it's cheap to keep resident in an agent's system
  prompt across a whole session.
- If a repo is large enough that the full manifest would blow this budget,
  support a **scoped manifest**: `get_manifest(scope: "src/api/**")`
  returns just the subgraph for that path prefix.

## Why this beats naive "send the whole repo"

- Naive approach: N files × average file size → often 100K+ tokens for a
  mid-size repo, most of it irrelevant to any single task.
- Manifest approach: ~1 line per file × a few hundred files → low thousands
  of tokens, then 1-5 full-file reads (only what's needed) for the actual
  task. This is where the "90%+ savings" claim comes from — it's the ratio
  of (manifest + few file reads) to (whole repo dump).

## Validation

All three are implemented and run in CI (`.github/workflows/ci.yml`):

- **Golden-file test** — `manifest::tests::markdown_golden_render` asserts the
  rendered markdown byte-exactly. Any change to rendering logic must update it
  deliberately, not accidentally.
- **Token budget** — `tests/manifest_budget.rs::fixture_manifest_stays_inside_its_token_budget`
  fails the build if the fixture repo's manifest exceeds its budget (measured
  value plus the 20% headroom specified here).
- **Per-file cost** — `manifest_cost_per_file_stays_flat` is the check that
  actually guards compression at scale: it bounds chars-per-indexed-file and
  pins the instructional footer as fixed overhead. A whole-repo compression
  *ratio* is not a useful assertion on the fixture, whose 27 stub files total
  ~2 KB — the fixed footer dominates it.

Token counts are exact, produced by `repo_graph::tokens::count_tokens` (a real
BPE, `o200k_base`) — the same function the benchmark uses, so the budget here
and the published figures cannot drift apart. See `BENCHMARK_REPORT.md` §3 for
why the previous `chars / 3.7` estimate was replaced and what it got wrong.

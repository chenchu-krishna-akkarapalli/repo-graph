# Logic: Token Optimization / Manifest Generation

Owns: turning the internal `Graph` into the compact, agent-facing manifest
that gets injected into a system prompt or returned from `get_manifest()`.
This is the whole point of the project — get this right and everything
upstream (walker, parsers, graph) is in service of it.

## Manifest schema (JSON, source of truth)

> **Schema version is a semver string, not an integer.** `manifest.schema_version`
> is `"1.2.0"` (`manifest::MANIFEST_SCHEMA_VERSION`) and is deliberately
> distinct from `graph.schema_version`, the integer identifying the on-disk
> `.repograph/graph.json` cache layout, which remains `1`.
>
> `1.2.0` adds ranking: `files[i].rank`, `rank_order`, `in_degree`, and
> `total_files`. `Graph::schema_version` stays `1` because `rank_score` /
> `rank_order` on `Node` are additive with serde defaults **and** recomputed on
> every load — an old cache is refreshed, never reported at rank 0.

```json
{
  "schema_version": "1.2.0",
  "generated_at": "2026-07-18T00:00:00Z",
  "root": "/path/to/repo",
  "total_files": 2,
  "files": [
    {
      "path": "src/lib/db.ts",
      "exports": ["db"],
      "depends_on": [],
      "route": null,
      "rank": 1.0,
      "rank_order": 1,
      "in_degree": 4
    },
    {
      "path": "src/pages/api/login.ts",
      "exports": [],
      "depends_on": ["src/lib/db.ts"],
      "route": "/api/login",
      "rank": 0.84507,
      "rank_order": 2,
      "in_degree": 0
    }
  ],
  "external_dependencies": ["react", "lodash"],
  "warnings": [
    { "path": "src/x.ts", "kind": "unresolved_dynamic" }
  ]
}
```

`total_files` is the in-scope count **before** `top_k` / `min_rank`. It exists
so an agent can never mistake a filtered map for a complete one — handing back
the top 20 files of 900 and letting the agent conclude the repo has 20 files is
the one way ranking can actively mislead.

## Ranking (`src-tauri/src/rank.rs`)

`rank` is weighted, personalized PageRank over the dependency edges,
normalized so the most central file is exactly `1.0`. Normalizing to the max
rather than leaving a raw probability is what makes `min_rank` mean the same
thing in a 40-file repo and a 40,000-file one.

Three departures from textbook PageRank, each covered by a test in
`rank::tests`:

| Departure | Why |
|:--|:--|
| Edge weights by kind — `contains` 1.5, `imports`/`require` 1.0, `references` 0.5, `route` 0.0 | A module-tree declaration is a stronger structural claim than an item-path reference. `route` edges are entry-point markers and already excluded from `dependencies_of`. |
| Personalized teleport: route handlers get a 3× prior | "Entry points matter more" is a statement about *prior* importance. Putting it in the teleport vector keeps the result a probability distribution and lets the boost propagate to the handler's dependencies. |
| Barrel contraction: an edge into a re-export barrel is redirected to what it re-exports, splitting the weight | A lower prior alone does **not** work — rank is the probability of *being* at a node, so a conduit accumulates everything routed through it regardless of prior. Contraction states the true relation: `import { Button } from './ui'` depends on `Button.tsx`. |

A barrel is detected conservatively: the filename matches the language's
convention (`index.*`, `mod.rs`, `__init__.py`), the file declares no symbols
of its own, and it has ≥2 outgoing edges. An `index.ts` containing real code is
not matched.

## Markdown rendering (what actually goes in the agent's prompt)

Rendered deterministically from the JSON — never hand-edited, never a
separate source of truth:

```markdown
## Project Architecture Map (Active Root: /repo)
- /src/lib/db.ts (Exports: db) (In: 4)
- /src/pages/api/login.ts (Route: /api/login) (Depends on: /src/lib/db.ts)

## Agent Instructions (payload format v1.2.0)
You are looking at a compressed map. Do NOT guess file contents.
Files are listed most-central first (dependency-graph rank). `(In: k)`: k files depend on this one. `[#N]`: repo-wide rank, shown only where it differs from list position. Narrow with `repograph_files(top_k: 20)`.
Use `repograph_explore(symbols)` for symbol slices with call graphs, or `repograph_node(path)` to read a whole file, before editing it.
For architecture questions use `repograph_explore(symbols, signature_only: true)` — declarations plus the call graph, without the bodies.
```

The tools were renamed: `get_manifest` → `repograph_files`, `read_file` →
`repograph_node`. `manifest::tests::markdown_golden_render` is the byte-exact
source of truth for this block.

## Compression rules

- One line per file, fixed format: `path (Exports: ...) (In: k) (Depends on: ...)`
  or `path (Route: ...) (Depends on: ...)` — omit empty fields entirely
  rather than printing `(Exports: none)`.
- **Ordered by rank, most central first.** This replaced directory-depth
  ordering in 1.2.0, which put `src/App.tsx` above `src/lib/db.ts` purely
  because it was shallower.
- **`[#N]` is emitted only where it disagrees with the line's own position** —
  i.e. under a scope, `sort_by`, or a collapsed list. It measured at 4 tokens
  per file, a quarter of a typical line, so paying it to restate the index
  would spend the budget on nothing.
- **`(In: k)` is emitted only at k ≥ 2.** `In: 1` says "used once", which a
  file with no marker already implies; at 2+ the file is shared, and that
  changes how an agent edits it.
- Over the line budget (default 500), the **lowest-ranked** files collapse
  into one summary line per directory. The pre-1.2.0 rule collapsed by "no
  exports and no route", which dropped a heavily-imported internal module
  while keeping a leaf exporting one unused constant.
- Always include `warnings` at the end as a short bullet list — agents
  should know when the map has known gaps (e.g. dynamic imports it
  couldn't resolve) rather than silently trusting an incomplete map.

## Ranking filters (`repograph_files`)

| Param | Effect |
|:--|:--|
| `top_k: usize` | Keep the K highest-ranked in-scope files. |
| `min_rank: f64` | Keep files scoring ≥ this. Normalized, so `0.1` is a reasonable "core only" cut. |
| `sort_by: "rank" \| "alphabetical" \| "depth"` | **Presentation only.** `top_k`/`min_rank` always select by rank — "the 20 most important files, listed alphabetically" is coherent; "the alphabetically first 20" is not. Unknown values fall back to `rank` rather than erroring. |

A `min_rank` that matches nothing in a non-empty repo returns a structured
`empty_rank_filter` error, not an empty map — otherwise it is
indistinguishable from an empty repository.

## Token budget targets

- Manifest should stay under ~2-3K tokens for a mid-size repo (a few
  hundred files) so it's cheap to keep resident in an agent's system
  prompt across a whole session.
- If a repo is large enough that the full manifest would blow this budget,
  scope it (`repograph_files(scope: "src/api/**")`) or rank-filter it
  (`repograph_files(top_k: 20)`). Scoping needs the agent to already know
  where to look; `top_k` does not, which is why it is the better first move
  on an unfamiliar repo.

## Why this beats naive "send the whole repo"

- Naive approach: N files × average file size → often 100K+ tokens for a
  mid-size repo, most of it irrelevant to any single task.
- Manifest approach: ~1 line per file × a few hundred files → low thousands
  of tokens, then 1-5 full-file reads (only what's needed) for the actual
  task. This is where the "90%+ savings" claim comes from — it's the ratio
  of (manifest + few file reads) to (whole repo dump).

## Search & Exploration Token Optimization (`repograph_search` & `repograph_explore`)

| Tool Method | Token Usage | Target Use Case |
|:---|:---|:---|
| `repograph_explore(symbols, signature_only: true)` | **~800 tokens** (92% savings) | Retrieve call graph, callers, and declaration signatures without body dumps. |
| `repograph_callers(symbol)` | **~300 tokens** | Find exact list of calling components without code bodies. |
| `repograph_node(path)` | **~200 tokens** | Read exact single-file definition. |
| `repograph_search(query, limit: 5)` | **~400 tokens** | Paginated symbol definition locator with 3-line context snippets. |

### Search Optimizations (`repograph_search`)
1. **Default `signature_only = true`:** Returns compact declaration heads (`signature`) rather than full 250+ line JSX/AST bodies (`content`), slashing response size by >90%.
2. **Default `limit = 10`:** Prevents high-frequency component references from overflowing model context windows.
3. **3-Line Grep Context Snippets:** Body matches extract a 3-line localized snippet (`extract_3line_snippet`) around the matching line instead of ingesting entire file ASTs.

## Validation

All three are implemented and run in CI (`.github/workflows/ci.yml`):

- **Golden-file test** — `manifest::tests::markdown_golden_render` asserts the
  rendered markdown byte-exactly. Any change to rendering logic must update it
  deliberately, not accidentally.
- **Token budget** — `tests/manifest_budget.rs::fixture_manifest_stays_inside_its_token_budget`
  fails the build if the fixture repo's manifest exceeds its budget (measured
  value plus the 20% headroom specified here).
- **Ranking** — `rank::tests` (11 tests) covers the hub/transitivity property,
  entry-point priors, barrel contraction including chained and cyclic barrels,
  cycle convergence, and that ranking is idempotent and independent of node
  order (otherwise the manifest is not reproducible).
- **Filter savings** — `tests/manifest_budget.rs::top_k_cuts_manifest_cost_in_proportion`
  asserts `top_k` actually cuts body cost in proportion to what it drops.
  Measured on the body, not the whole payload: the fixed footer dominates a
  25-file fixture, so a whole-payload ratio would measure the footer.
- **Per-file cost** — `manifest_cost_per_file_stays_flat` is the check that
  actually guards compression at scale: it bounds chars-per-indexed-file and
  pins the instructional footer as fixed overhead. A whole-repo compression
  *ratio* is not a useful assertion on the fixture, whose 27 stub files total
  ~2 KB — the fixed footer dominates it.

Token counts are exact, produced by `repo_graph::tokens::count_tokens` (a real
BPE, `o200k_base`) — the same function the benchmark uses, so the budget here
and the published figures cannot drift apart. See `BENCHMARK_REPORT.md` §3 for
why the previous `chars / 3.7` estimate was replaced and what it got wrong.

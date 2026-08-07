# Logic: Graph Engine

Owns: turning a raw file list + extracted edges into a queryable,
serializable dependency graph.

## Inputs

- `FileEntry[]` from the walker: `{path, size_bytes, extension, language}`
- `Edge[]` from language parsers: `{from_path, to_path, kind}` where `kind`
  is one of `import | require | mod | use | route`

## Core data structure

```
Graph {
  schema_version: u32,
  nodes: HashMap<PathBuf, Node>,
  edges: Vec<Edge>,
}

Node {
  path: PathBuf,
  size_bytes: u64,
  language: Language,
  exports: Vec<String>,
  routes: Vec<String>,       // populated only for route-marked files
  in_degree: u32,            // how many files depend on this one
  out_degree: u32,           // how many files this one depends on
}
```

## Build steps

1. **Ingest nodes** from the walker output. Reject nodes over a size
   threshold from full parsing (still index them, just skip AST parse) to
   avoid pathological slowdowns on generated/minified files.
2. **Ingest edges** from parser output. Resolve every edge's `to_path`
   against the repo root — reject/flag edges that resolve outside the repo
   (external package imports) and store them separately as
   `external_dependencies` rather than graph edges, since agents don't need
   to traverse into `node_modules`.
3. **Compute degrees.** `in_degree`/`out_degree` per node, used for surfacing
   "hub" files (high in-degree = risky to change) in the visualizer.
4. **Detect cycles** (optional, informational only) — surfaced in the UI as
   a warning, not blocking.
5. **Serialize** to JSON for the Tauri bridge and for the MCP server.

## Query operations required by the agent API

- `find_dependents(path)` — all nodes with an edge `to_path == path`
- `find_dependencies(path)` — all nodes with an edge `from_path == path`
- `subgraph_for(paths[])` — induced subgraph over a set of files, used when
  rendering "what would be affected if I change these N files"

## Performance targets

- Full graph build for a 10k-file repo: under 2 seconds on a typical
  laptop, using the multi-threaded walker + parser pool.
- Incremental update (single file changed): re-parse only that file, patch
  its node + edges, don't rebuild the whole graph.

## Edge cases

- **Barrel files** (`index.ts` re-exporting from multiple modules): treat as
  a normal node; its high out-degree is informative, not a bug.
- **Dynamic imports** (`import(...)` at runtime, computed paths): flag as
  `unresolved_dynamic` edges rather than silently dropping them, so the
  manifest can note "this file has dynamic imports the map can't fully
  resolve."
- **Circular dependencies:** allowed in the graph structure; only flagged as
  a UI warning.

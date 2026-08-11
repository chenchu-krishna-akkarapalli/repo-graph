export interface ExtractedSymbol {
  name: string
  kind: string
  start_line: number
  end_line: number
}

export interface SymbolEdge {
  from_symbol: string
  to_symbol: string
  kind: string
  provenance: string
  wiring_site: string | null
}

export interface GraphNode {
  path: string
  size_bytes: number
  language: string
  exports: string[]
  routes: string[]
  in_degree: number
  out_degree: number
  symbols: ExtractedSymbol[]
  /**
   * Normalized dependency-graph centrality in (0, 1], 1.0 for the most central
   * file. Computed by the Rust `rank` module on every graph load, so a cache
   * written before ranking existed is refreshed rather than reported as 0.
   *
   * Optional here because `RepoGraph` is also constructed in tests and older
   * caches deserialize without it.
   */
  rank_score?: number
  /** Dense 1-based position in the ranking. */
  rank_order?: number
}

/**
 * Wire values, not Rust variant names.
 *
 * `graph::EdgeKind` renames on serialization (`Import → "imports"`,
 * `Mod → "contains"`, `Use → "references"`), so three of the names previously
 * listed here — `'import'`, `'mod'`, `'use'` — never appear in a payload,
 * while the three that do were missing. Nothing compared against them yet
 * (only `'route'`, which was correct), but `e.kind === 'import'` would have
 * silently been dead code.
 *
 * The Rust side keeps `alias` entries for the old spellings so existing
 * `.repograph/graph.json` caches still deserialize.
 */
export type EdgeKind = 'imports' | 'require' | 'contains' | 'references' | 'route'

export interface GraphEdge {
  from_path: string
  to_path: string
  kind: EdgeKind
}

export interface GraphWarning {
  path: string
  kind: string
}

export interface RepoGraph {
  schema_version: number
  nodes: GraphNode[]
  edges: GraphEdge[]
  external_dependencies: string[]
  warnings: GraphWarning[]
  symbol_edges?: SymbolEdge[]
}

export type SyncStatus = 'indexing' | 'synced' | 'stale' | 'updated'

/** One end of a call-graph edge, from the `get_symbol_call_graph` command. */
export interface CallGraphNode {
  file_path: string
  symbol_name: string
  kind: string
}

export interface CallGraph {
  callers: CallGraphNode[]
  callees: CallGraphNode[]
}

/** One row from the `search_symbols` command. */
export interface SymbolSearchResult {
  name: string
  file_path: string
  content: string
}

/** Explorer tree node returned by the `read_directory_tree` IPC command. */
export interface FileTreeNode {
  name: string
  path: string
  is_dir: boolean
  children: FileTreeNode[]
}

export interface AgentActivity {
  id: string
  /** Absent when the agent queried by path alone. */
  symbol: string | null
  /** Absent when the agent queried by bare symbol name. */
  path: string | null
  action: string
  timestamp: number
}

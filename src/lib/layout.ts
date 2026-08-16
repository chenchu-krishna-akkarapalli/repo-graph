import type { Edge, Node } from 'reactflow'
import type { RepoGraph, ExtractedSymbol } from '../types'
import { neighborAttr } from './hoverHighlight'
import { detectCommunities, COMMUNITY_PALETTE } from './community'

/** Fitts's Law minimums — nodes must stay clickable when zoomed out. */
export const NODE_WIDTH = 176
export const NODE_HEIGHT = 48

const COLUMN_GAP = 340
const ROW_GAP = 96

/**
 * How much a top-ranked node widens over an unranked one.
 *
 * Width rather than height: height is load-bearing here — an expanded node's
 * height is `48 + symbols × 44` and its symbol children are positioned against
 * those exact offsets, so scaling it would misalign them. Width is free.
 *
 * The cap keeps the widest node (176 × 1.35 ≈ 238px) inside `COLUMN_GAP`, so
 * columns can never collide however the ranks fall.
 */
const RANK_WIDTH_GAIN = 0.35

/** Node width for a given normalized rank. Exported for the centering math. */
export function rankedWidth(rank: number): number {
  const clamped = Math.max(0, Math.min(1, rank))
  return Math.round(NODE_WIDTH * (1 + RANK_WIDTH_GAIN * clamped))
}

export interface FileNodeData {
  path: string
  name: string
  language: string
  exports: string[]
  routes: string[]
  dir: string
  isExpanded?: boolean
  symbols?: ExtractedSymbol[]
  /** `|a|b|` adjacency string consumed by the CSS hover rules (see
   *  `hoverHighlight.ts`) — computed once per layout, never per mouse move. */
  neighbors: string
  /** Normalized graph centrality in [0, 1]. `0` when the backend predates
   *  ranking, which reads as "unranked" and renders at the base width. */
  rank: number
  /** Dense 1-based repo-wide rank; `0` when unranked. */
  rankOrder: number
  /** In-repo files depending on this one. Route edges excluded, matching the
   *  manifest's `(In: k)` — the same number should not mean two things. */
  dependents: number
  domainName?: string
  domainColor?: string
  domainColorHex?: string
  domainBg?: string
  domainBorder?: string
}

export interface FolderNodeData {
  dir: string
  fileCount: number
}

export interface SymbolNodeData {
  name: string
  kind: string
  parentFile: string
  neighbors: string
}

/** Payload of any node `buildFlow` emits, discriminated by `Node.type`. */
export type FlowNodeData = FileNodeData | FolderNodeData | SymbolNodeData

/**
 * Deterministic layered layout: x = directory depth or domain cluster, y = slot within the
 * column (grouped by directory/domain, then rank) so the graph reads cleanly
 * left-to-right. Collapsed directories replace their files with one folder
 * node; edges re-point to it (deduped).
 */
export function buildFlow(
  graph: RepoGraph,
  languageFilters: ReadonlySet<string>,
  collapsedDirs: ReadonlySet<string>,
  expandedFiles: ReadonlySet<string>,
  minRank = 0,
  densityMode: 'full' | 'core' | 'domains' = 'full',
): { nodes: Node<FlowNodeData>[]; edges: Edge[] } {
  let visible = graph.nodes.filter(
    (n) =>
      languageFilters.has(normalizeLanguage(n.language)) &&
      (n.rank_score ?? 0) >= minRank,
  )

  if (densityMode === 'core') {
    visible = visible.filter(
      (n) =>
        ((n.rank_order ?? 0) > 0 && (n.rank_order ?? 0) <= 30) ||
        expandedFiles.has(n.path),
    )
  }

  const { nodeCommunityMap, communities } = detectCommunities(graph)

  // Adjacency baked into the node payload so hover highlighting is a pure
  // CSS attribute match instead of a per-node store lookup.
  const neighborSets = new Map<string, Set<string>>()
  const dependentCounts = new Map<string, number>()
  for (const e of graph.edges) {
    addNeighbor(neighborSets, e.from_path, e.to_path)
    addNeighbor(neighborSets, e.to_path, e.from_path)
    if (e.kind !== 'route') {
      dependentCounts.set(e.to_path, (dependentCounts.get(e.to_path) ?? 0) + 1)
    }
  }

  /** File path → its representative id (own path, or collapsed folder id). */
  const representative = new Map<string, string>()
  const folderCounts = new Map<string, number>()

  for (const node of visible) {
    const dir = dirOf(node.path)
    const collapsedAncestor = findCollapsedAncestor(dir, collapsedDirs)
    if (collapsedAncestor !== null) {
      representative.set(node.path, `folder:${collapsedAncestor}`)
      folderCounts.set(collapsedAncestor, (folderCounts.get(collapsedAncestor) ?? 0) + 1)
    } else {
      representative.set(node.path, node.path)
    }
  }

  // Layout slots per depth column.
  const currentYMap = new Map<number, number>()
  const nodes: Node<FlowNodeData>[] = []

  const fileNodes = visible
    .filter((n) => representative.get(n.path) === n.path)
    .sort((a, b) => a.path.localeCompare(b.path))
  for (const n of fileNodes) {
    const communityId = nodeCommunityMap.get(n.path) ?? 0
    const community = communities.find((c) => c.id === communityId)
    const depth = densityMode === 'domains' ? communityId : n.path.split('/').length - 1
    const isExpanded = expandedFiles.has(n.path) && n.symbols && n.symbols.length > 0
    const childCount = isExpanded ? n.symbols.length : 0
    const nodeHeight = isExpanded ? 48 + childCount * 44 + 10 : NODE_HEIGHT
    const y = currentYMap.get(depth) ?? 0
    currentYMap.set(depth, y + nodeHeight + ROW_GAP)
    const neighbors = neighborAttr(neighborSets.get(n.path) ?? [])
    const rank = n.rank_score ?? 0
    const nodeWidth = rankedWidth(rank)

    nodes.push({
      id: n.path,
      type: 'file',
      position: { x: depth * COLUMN_GAP, y },
      data: {
        path: n.path,
        name: n.path.split('/').pop() ?? n.path,
        language: normalizeLanguage(n.language),
        exports: n.exports,
        routes: n.routes,
        dir: dirOf(n.path),
        isExpanded,
        symbols: n.symbols,
        neighbors,
        rank,
        rankOrder: n.rank_order ?? 0,
        dependents: dependentCounts.get(n.path) ?? 0,
        domainName: community?.name,
        domainColor: community?.colorClass,
        domainColorHex: community ? COMMUNITY_PALETTE[community.id % COMMUNITY_PALETTE.length]?.color : undefined,
        domainBg: community?.bgClass,
        domainBorder: community?.borderClass,
      },
      style: { width: nodeWidth, height: nodeHeight },
    })

    if (isExpanded) {
      n.symbols.forEach((sym, index) => {
        nodes.push({
          id: `${n.path}#${sym.name}`,
          type: 'symbol',
          parentNode: n.path,
          extent: 'parent',
          position: { x: 10, y: 48 + index * 44 },
          // Tracks the parent's rank-scaled width so children never overflow.
          style: { width: nodeWidth - 20, height: 34 },
          // Symbols inherit the parent file's identity so they dim/light
          // together with it under the same CSS rule.
          data: { name: sym.name, kind: sym.kind, parentFile: n.path, neighbors },
        })
      })
    }
  }

  for (const [dir, fileCount] of [...folderCounts.entries()].sort()) {
    const depth = dir === '' ? 0 : dir.split('/').length
    const y = currentYMap.get(depth) ?? 0
    currentYMap.set(depth, y + NODE_HEIGHT + ROW_GAP)
    nodes.push({
      id: `folder:${dir}`,
      type: 'folder',
      position: { x: depth * COLUMN_GAP, y },
      data: { dir, fileCount },
      style: { width: NODE_WIDTH, height: NODE_HEIGHT },
    })
  }

  const edges: Edge[] = []
  const seen = new Set<string>()
  for (const e of graph.edges) {
    const source = representative.get(e.from_path)
    const target = representative.get(e.to_path)
    if (!source || !target || source === target) continue
    const id = `${source}->${target}`
    if (seen.has(id)) continue
    seen.add(id)
    edges.push({
      id,
      source,
      target,
      type: 'highlight',
      data: { from_path: e.from_path, to_path: e.to_path, kind: e.kind },
    })
  }

  // Symbol-level call edges (only if both parent files are expanded and visible).
  //
  // The `expandedFiles.size` guard matters: `symbol_edges` is the largest
  // array in the payload, and this walked all of it on every relayout —
  // including every language-filter and collapse toggle, when by definition
  // no symbol edge could be drawn.
  if (graph.symbol_edges && expandedFiles.size > 0) {
    for (const se of graph.symbol_edges) {
      const fromIdx = se.from_symbol.indexOf('#')
      const toIdx = se.to_symbol.indexOf('#')
      if (fromIdx === -1 || toIdx === -1) continue

      const fromFile = se.from_symbol.substring(0, fromIdx)
      const toFile = se.to_symbol.substring(0, toIdx)

      if (representative.get(fromFile) === fromFile && representative.get(toFile) === toFile) {
        if (expandedFiles.has(fromFile) && expandedFiles.has(toFile)) {
          const id = `${se.from_symbol}->${se.to_symbol}`
          if (seen.has(id)) continue
          seen.add(id)
          edges.push({
            id,
            source: se.from_symbol,
            target: se.to_symbol,
            type: 'highlight',
            data: {
              from_path: fromFile,
              to_path: toFile,
              kind: se.kind,
              provenance: se.provenance,
              wiring_site: se.wiring_site,
              isSymbolEdge: true,
            },
          })
        }
      }
    }
  }

  return { nodes, edges }
}

/**
 * Languages the filter bar can distinguish. This used to be
 * `javascript | python | rust | other`, which collapsed the other sixteen
 * supported languages into one indistinguishable bucket — a Go/Java/C# repo
 * rendered as a wall of "other" with no way to filter within it.
 *
 * Keep in sync with `walker::language_for`.
 */
export const KNOWN_LANGUAGES = [
  'javascript',
  'python',
  'rust',
  'go',
  'java',
  'kotlin',
  'csharp',
  'c',
  'cpp',
  'swift',
  'php',
  'sfc',
  'html',
  'sql',
  'prisma',
  'graphql',
  'dockerfile',
  'markdown',
  'other',
] as const

const KNOWN_LANGUAGE_SET: ReadonlySet<string> = new Set(KNOWN_LANGUAGES)

export function normalizeLanguage(language: string): string {
  return KNOWN_LANGUAGE_SET.has(language) ? language : 'other'
}

function addNeighbor(map: Map<string, Set<string>>, key: string, value: string) {
  const bag = map.get(key)
  if (bag) bag.add(value)
  else map.set(key, new Set([value]))
}

export function dirOf(path: string): string {
  const idx = path.lastIndexOf('/')
  return idx === -1 ? '' : path.slice(0, idx)
}

function findCollapsedAncestor(
  dir: string,
  collapsedDirs: ReadonlySet<string>,
): string | null {
  let current = dir
  let found: string | null = null
  while (current !== '') {
    if (collapsedDirs.has(current)) found = current
    const idx = current.lastIndexOf('/')
    current = idx === -1 ? '' : current.slice(0, idx)
  }
  return found
}

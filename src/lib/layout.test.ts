import { describe, expect, it } from 'vitest'
import type { RepoGraph } from '../types'
import {
  buildFlow,
  dirOf,
  KNOWN_LANGUAGES,
  NODE_WIDTH,
  normalizeLanguage,
  rankedWidth,
  type FileNodeData,
  type FolderNodeData,
} from './layout'

const ALL_LANGUAGES: ReadonlySet<string> = new Set(KNOWN_LANGUAGES)

function graph(partial: Partial<RepoGraph> = {}): RepoGraph {
  return {
    schema_version: 1,
    nodes: [],
    edges: [],
    external_dependencies: [],
    warnings: [],
    symbol_edges: [],
    ...partial,
  } as RepoGraph
}

function node(path: string, language = 'javascript', symbols: RepoGraph['nodes'][number]['symbols'] = []) {
  return {
    path,
    size_bytes: 0,
    language,
    exports: [],
    routes: [],
    in_degree: 0,
    out_degree: 0,
    symbols,
  }
}

describe('normalizeLanguage', () => {
  it('keeps every supported language distinct', () => {
    // These used to collapse into a single "other" bucket, so a Go or Java
    // repo rendered as an unfilterable wall.
    for (const lang of ['go', 'java', 'kotlin', 'csharp', 'swift', 'php', 'sql']) {
      expect(normalizeLanguage(lang)).toBe(lang)
    }
  })

  it('still funnels genuinely unknown languages to other', () => {
    expect(normalizeLanguage('brainfuck')).toBe('other')
    expect(normalizeLanguage('')).toBe('other')
  })
})

describe('buildFlow', () => {
  it('hides nodes whose language is filtered out', () => {
    const g = graph({ nodes: [node('a.ts', 'javascript'), node('b.go', 'go')] })
    const { nodes } = buildFlow(g, new Set(['javascript']), new Set(), new Set())
    expect(nodes.map((n) => n.id)).toEqual(['a.ts'])
  })

  it('collapses a directory into one folder node and repoints its edges', () => {
    const g = graph({
      nodes: [node('src/a.ts'), node('src/b.ts'), node('main.ts')],
      edges: [{ from_path: 'main.ts', to_path: 'src/a.ts', kind: 'imports' as const }],
    })
    const { nodes, edges } = buildFlow(g, ALL_LANGUAGES, new Set(['src']), new Set())

    const folder = nodes.find((n) => n.id === 'folder:src')
    expect((folder?.data as FolderNodeData).fileCount).toBe(2)
    expect(nodes.some((n) => n.id === 'src/a.ts')).toBe(false)
    expect(edges).toHaveLength(1)
    expect(edges[0].target).toBe('folder:src')
  })

  it('deduplicates edges that collapse onto the same pair', () => {
    const g = graph({
      nodes: [node('src/a.ts'), node('src/b.ts'), node('main.ts')],
      edges: [
        { from_path: 'main.ts', to_path: 'src/a.ts', kind: 'imports' as const },
        { from_path: 'main.ts', to_path: 'src/b.ts', kind: 'imports' },
      ],
    })
    const { edges } = buildFlow(g, ALL_LANGUAGES, new Set(['src']), new Set())
    expect(edges).toHaveLength(1)
  })

  it('skips the symbol-edge pass entirely when nothing is expanded', () => {
    // `symbol_edges` is the largest array in the payload; walking it on every
    // filter or collapse toggle was pure waste, since no symbol node exists.
    const symbolEdges = Array.from({ length: 500 }, (_, i) => ({
      from_symbol: `a.ts#s${i}`,
      to_symbol: `b.ts#t${i}`,
      kind: 'calls',
      provenance: 'ast',
      wiring_site: null,
    }))
    const g = graph({ nodes: [node('a.ts'), node('b.ts')], symbol_edges: symbolEdges })

    const collapsed = buildFlow(g, ALL_LANGUAGES, new Set(), new Set())
    expect(collapsed.edges).toHaveLength(0)
  })

  it('draws symbol edges only when both endpoint files are expanded', () => {
    const symbols = [{ name: 'go', kind: 'function', start_line: 1, end_line: 2 }]
    const g = graph({
      nodes: [node('a.ts', 'javascript', symbols), node('b.ts', 'javascript', symbols)],
      symbol_edges: [
        {
          from_symbol: 'a.ts#go',
          to_symbol: 'b.ts#go',
          kind: 'calls',
          provenance: 'ast',
          wiring_site: null,
        },
      ],
    })

    const onlyOne = buildFlow(g, ALL_LANGUAGES, new Set(), new Set(['a.ts']))
    expect(onlyOne.edges.some((e) => e.data?.isSymbolEdge)).toBe(false)

    const both = buildFlow(g, ALL_LANGUAGES, new Set(), new Set(['a.ts', 'b.ts']))
    expect(both.edges.some((e) => e.data?.isSymbolEdge)).toBe(true)
  })
})

describe('rank rendering', () => {
  function ranked(path: string, rank_score: number, rank_order: number) {
    return { ...node(path), rank_score, rank_order }
  }

  it('scales node width with rank and keeps it inside the column gap', () => {
    const g = graph({ nodes: [ranked('hub.ts', 1, 1), ranked('leaf.ts', 0.1, 2)] })
    const { nodes } = buildFlow(g, ALL_LANGUAGES, new Set(), new Set())
    const hub = nodes.find((n) => n.id === 'hub.ts')!
    const leaf = nodes.find((n) => n.id === 'leaf.ts')!
    expect(hub.style?.width).toBeGreaterThan(leaf.style?.width as number)
    // Columns are 340px apart; a wider node than that would collide.
    expect(hub.style?.width as number).toBeLessThan(340)
  })

  it('renders an unranked graph at the base width instead of collapsing it', () => {
    // A `.repograph/graph.json` written before ranking existed has no
    // rank fields at all; it must still lay out normally.
    const g = graph({ nodes: [node('a.ts')] })
    const { nodes } = buildFlow(g, ALL_LANGUAGES, new Set(), new Set())
    expect(nodes).toHaveLength(1)
    expect(nodes[0].style?.width).toBe(NODE_WIDTH)
    expect((nodes[0].data as FileNodeData).rankOrder).toBe(0)
  })

  it('clamps out-of-range ranks rather than producing an absurd width', () => {
    expect(rankedWidth(0)).toBe(NODE_WIDTH)
    expect(rankedWidth(-5)).toBe(NODE_WIDTH)
    expect(rankedWidth(99)).toBe(rankedWidth(1))
  })

  it('hides nodes below minRank', () => {
    const g = graph({ nodes: [ranked('hub.ts', 1, 1), ranked('leaf.ts', 0.1, 2)] })
    const { nodes } = buildFlow(g, ALL_LANGUAGES, new Set(), new Set(), 0.5)
    expect(nodes.map((n) => n.id)).toEqual(['hub.ts'])
  })

  it('leaves an unranked graph untouched at the default minRank', () => {
    // The regression that would matter most: defaulting to a filter that
    // treats "no rank" as "rank 0" and blanks the canvas for old caches.
    const g = graph({ nodes: [node('a.ts'), node('b.ts')] })
    expect(buildFlow(g, ALL_LANGUAGES, new Set(), new Set()).nodes).toHaveLength(2)
  })

  it('counts dependents excluding route markers, matching the manifest', () => {
    const g = graph({
      nodes: [node('db.ts'), node('a.ts'), node('b.ts')],
      edges: [
        { from_path: 'a.ts', to_path: 'db.ts', kind: 'imports' as const },
        { from_path: 'b.ts', to_path: 'db.ts', kind: 'imports' as const },
        { from_path: 'a.ts', to_path: 'db.ts', kind: 'route' as const },
      ],
    })
    const { nodes } = buildFlow(g, ALL_LANGUAGES, new Set(), new Set())
    const db = nodes.find((n) => n.id === 'db.ts')!
    expect((db.data as FileNodeData).dependents).toBe(2)
  })

  it('sizes expanded symbol children to the parent rank-scaled width', () => {
    const symbols = [{ name: 'go', kind: 'function', start_line: 1, end_line: 2 }]
    const g = graph({ nodes: [{ ...node('a.ts', 'javascript', symbols), rank_score: 1, rank_order: 1 }] })
    const { nodes } = buildFlow(g, ALL_LANGUAGES, new Set(), new Set(['a.ts']))
    const parent = nodes.find((n) => n.id === 'a.ts')!
    const child = nodes.find((n) => n.id === 'a.ts#go')!
    expect(child.style?.width).toBe((parent.style?.width as number) - 20)
  })
})

describe('dirOf', () => {
  it('returns the empty string for a root-level file', () => {
    expect(dirOf('main.ts')).toBe('')
    expect(dirOf('src/lib/a.ts')).toBe('src/lib')
  })
})

import { describe, expect, it } from 'vitest'
import type { RepoGraph } from '../types'
import { buildFlow, dirOf, KNOWN_LANGUAGES, normalizeLanguage, type FolderNodeData } from './layout'

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

describe('dirOf', () => {
  it('returns the empty string for a root-level file', () => {
    expect(dirOf('main.ts')).toBe('')
    expect(dirOf('src/lib/a.ts')).toBe('src/lib')
  })
})

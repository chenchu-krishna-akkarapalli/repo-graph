import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { AgentActivity, RepoGraph } from './types'
import { agentActivityTargets, primarySelection, rankedFiles, useGraphStore } from './store'
import { cleanUncPath } from './lib/fileTree'

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

function node(path: string, symbolNames: string[] = []) {
  return {
    path,
    size_bytes: 0,
    language: 'javascript',
    exports: [],
    routes: [],
    in_degree: 0,
    out_degree: 0,
    symbols: symbolNames.map((name) => ({
      name,
      kind: 'function',
      start_line: 1,
      end_line: 2,
    })),
  }
}

const activity = (partial: Partial<AgentActivity> = {}): AgentActivity =>
  ({
    id: 'a1',
    action: 'explore',
    path: null,
    symbol: null,
    timestamp: Date.now(),
    ...partial,
  }) as AgentActivity

describe('rankedFiles', () => {
  const ranked = (path: string, rank_order: number, rank_score: number) => ({
    ...node(path),
    rank_order,
    rank_score,
  })

  it('orders by the backend rank, not by insertion or score re-derivation', () => {
    const g = graph({
      nodes: [ranked('c.ts', 3, 0.2), ranked('a.ts', 1, 1), ranked('b.ts', 2, 0.6)],
    })
    expect(rankedFiles(g).map((n) => n.path)).toEqual(['a.ts', 'b.ts', 'c.ts'])
  })

  it('drops unranked nodes instead of seating them at #1', () => {
    // `rank_order: 0` sorts first numerically — an old cache would otherwise
    // fill the entire "core files" list with unranked entries.
    const g = graph({ nodes: [node('old.ts'), ranked('hub.ts', 1, 1)] })
    expect(rankedFiles(g).map((n) => n.path)).toEqual(['hub.ts'])
  })

  it('applies the limit and tolerates a null graph', () => {
    const g = graph({ nodes: [ranked('a.ts', 1, 1), ranked('b.ts', 2, 0.5)] })
    expect(rankedFiles(g, 1).map((n) => n.path)).toEqual(['a.ts'])
    expect(rankedFiles(null)).toEqual([])
  })

  it('does not mutate the caller\'s node array', () => {
    // `Array.prototype.sort` is in-place; sorting `graph.nodes` directly would
    // reorder the store's own graph as a side effect of rendering a sidebar.
    const g = graph({ nodes: [ranked('c.ts', 3, 0.2), ranked('a.ts', 1, 1)] })
    rankedFiles(g)
    expect(g.nodes.map((n) => n.path)).toEqual(['c.ts', 'a.ts'])
  })
})

describe('setMinRank', () => {
  it('clamps to [0, 1] so the canvas can never be blanked by a bad value', () => {
    const { setMinRank } = useGraphStore.getState()
    setMinRank(0.4)
    expect(useGraphStore.getState().minRank).toBe(0.4)
    setMinRank(-1)
    expect(useGraphStore.getState().minRank).toBe(0)
    setMinRank(7)
    expect(useGraphStore.getState().minRank).toBe(1)
    setMinRank(0)
  })
})

describe('agentActivityTargets', () => {
  it('uses the path directly when one is given', () => {
    expect(agentActivityTargets(null, activity({ path: 'src/a.ts' }))).toEqual(['src/a.ts'])
    expect(agentActivityTargets(null, activity({ path: 'src/a.ts', symbol: 'go' }))).toEqual([
      'src/a.ts',
      'src/a.ts#go',
    ])
  })

  it('locates a bare symbol across the graph', () => {
    const g = graph({ nodes: [node('a.ts', ['go']), node('b.ts', ['stop']), node('c.ts', ['go'])] })
    expect(agentActivityTargets(g, activity({ symbol: 'go' }))).toEqual([
      'a.ts',
      'a.ts#go',
      'c.ts',
      'c.ts#go',
    ])
  })

  it('returns nothing when there is no graph or no locator', () => {
    expect(agentActivityTargets(null, activity({ symbol: 'go' }))).toEqual([])
    expect(agentActivityTargets(graph(), activity())).toEqual([])
  })
})

describe('addAgentActivity', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    useGraphStore.getState().clearAgentActivity()
  })

  it('highlights the touched node then releases it after the pulse', () => {
    const { addAgentActivity } = useGraphStore.getState()
    addAgentActivity(activity({ path: 'src/a.ts' }))

    expect(useGraphStore.getState().activeAgentTargets.has('src/a.ts')).toBe(true)
    vi.advanceTimersByTime(2000)
    expect(useGraphStore.getState().activeAgentTargets.has('src/a.ts')).toBe(false)
  })

  it('keeps the activity log bounded', () => {
    const { addAgentActivity } = useGraphStore.getState()
    for (let i = 0; i < 150; i++) {
      addAgentActivity(activity({ id: `a${i}`, path: `f${i}.ts` }))
    }
    expect(useGraphStore.getState().agentActivity).toHaveLength(100)
    // Newest first.
    expect(useGraphStore.getState().agentActivity[0].id).toBe('a149')
  })

  it('cancels in-flight pulse timers when cleared', () => {
    const { addAgentActivity, clearAgentActivity } = useGraphStore.getState()
    addAgentActivity(activity({ path: 'src/a.ts' }))
    clearAgentActivity()

    expect(useGraphStore.getState().activeAgentTargets.size).toBe(0)
    // The pending timeout must not resurrect or mutate state after clearing.
    vi.advanceTimersByTime(5000)
    expect(useGraphStore.getState().activeAgentTargets.size).toBe(0)
    expect(useGraphStore.getState().agentActivity).toHaveLength(0)
  })
})

describe('simulateImpact', () => {
  it('walks the dependents chain transitively', () => {
    useGraphStore.setState({
      dependentsOf: new Map([
        ['a.ts', ['b.ts']],
        ['b.ts', ['c.ts']],
      ]),
    })
    useGraphStore.getState().simulateImpact('a.ts')

    const { impactSet, impactSource, sidebarTab } = useGraphStore.getState()
    expect([...impactSet].sort()).toEqual(['b.ts', 'c.ts'])
    expect(impactSource).toBe('a.ts')
    expect(sidebarTab).toBe('impact')
  })

  it('terminates on a dependency cycle', () => {
    useGraphStore.setState({
      dependentsOf: new Map([
        ['a.ts', ['b.ts']],
        ['b.ts', ['a.ts']],
      ]),
    })
    useGraphStore.getState().simulateImpact('a.ts')
    expect([...useGraphStore.getState().impactSet].sort()).toEqual(['a.ts', 'b.ts'])
  })
})

describe('setIndexProgress', () => {
  it('reports zero ETA once complete', () => {
    useGraphStore.getState().setIndexProgress({
      phase: 'complete',
      files_total: 10,
      files_processed: 10,
      bytes_total: 100,
      bytes_processed: 100,
    })
    expect(useGraphStore.getState().indexProgress.etaSeconds).toBe(0)
  })

  it('has no ETA before a speed sample exists', () => {
    useGraphStore.setState({
      indexProgress: {
        phase: 'idle',
        filesTotal: 0,
        filesProcessed: 0,
        bytesTotal: 0,
        bytesProcessed: 0,
        speedFilesPerSec: 0,
        etaSeconds: null,
      },
    })
    useGraphStore.getState().setIndexProgress({
      phase: 'parsing',
      files_total: 100,
      files_processed: 1,
      bytes_total: 1000,
      bytes_processed: 10,
    })
    expect(useGraphStore.getState().indexProgress.etaSeconds).toBeNull()
  })
})

describe('primarySelection', () => {
  it('returns the most recently added path, or null when empty', () => {
    expect(primarySelection(new Set())).toBeNull()
    expect(primarySelection(new Set(['a.ts', 'b.ts']))).toBe('b.ts')
  })
})

describe('graphCache persistence & auto-rehydration', () => {
  it('persists and restores graph cache successfully', async () => {
    const { savePersistedGraph, loadPersistedGraph, clearPersistedGraph } = await import('./lib/graphCache')
    clearPersistedGraph()
    expect(loadPersistedGraph()).toBeNull()

    const mockGraph = graph({
      nodes: [node('src/main.ts', ['runApp'])],
    })
    const saved = savePersistedGraph('/project/root', mockGraph)
    expect(saved).toBe(true)

    const loaded = loadPersistedGraph('/project/root')
    expect(loaded).not.toBeNull()
    expect(loaded?.root).toBe('/project/root')
    expect(loaded?.graph.nodes).toHaveLength(1)
    expect(loaded?.graph.nodes[0].path).toBe('src/main.ts')

    // Root mismatch guard
    expect(loadPersistedGraph('/other/workspace')).toBeNull()

    clearPersistedGraph()
    expect(loadPersistedGraph()).toBeNull()
  })

  it('handles massive 5,000-node graph scaling without crashing or leaking state', async () => {
    const largeNodes = Array.from({ length: 5000 }, (_, i) => ({
      ...node(`src/modules/module_${i}.ts`, [`func_${i}`, `helper_${i}`]),
      rank_order: i + 1,
      rank_score: 1 / (i + 1),
    }))
    const largeEdges = Array.from({ length: 8000 }, (_, i) => ({
      from_path: `src/modules/module_${i % 5000}.ts`,
      to_path: `src/modules/module_${(i * 3 + 1) % 5000}.ts`,
      kind: 'imports' as const,
    }))

    const largeGraph = graph({
      nodes: largeNodes,
      edges: largeEdges,
    })

    const started = performance.now()
    useGraphStore.setState({
      graph: largeGraph,
      status: 'synced',
    })

    const ranked = rankedFiles(largeGraph, 50)
    expect(ranked).toHaveLength(50)
    expect(ranked[0].path).toBe('src/modules/module_0.ts')
    const elapsed = performance.now() - started
    expect(elapsed).toBeLessThan(1000) // Must process under 1 second
  })

  it('safely handles localStorage quota overflow and corrupted cache', async () => {
    const { savePersistedGraph, loadPersistedGraph } = await import('./lib/graphCache')
    
    // Simulate corrupted JSON in localStorage
    localStorage.setItem('repograph:cached_graph_data', '{ corrupted-json... ')
    expect(loadPersistedGraph()).toBeNull()

    // Simulate quota error in setItem
    const spy = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('QuotaExceededError: DOM Exception 22')
    })
    const safeSaved = savePersistedGraph('/quota/root', graph({ nodes: [node('test.ts')] }))
    expect(safeSaved).toBe(false)
    spy.mockRestore()
  })

  it('correctly applies 1-click Agent Presets', () => {
    const testGraph = graph({
      nodes: [
        { ...node('src/auth/service.ts'), routes: ['POST /auth/login'], rank_score: 0.9 },
        node('src/auth/handler.ts'),
        node('src/utils.ts'),
      ],
      edges: [
        { from_path: 'src/auth/handler.ts', to_path: 'src/auth/service.ts', kind: 'imports' },
        { from_path: 'src/auth/service.ts', to_path: 'src/utils.ts', kind: 'imports' },
      ],
    })

    useGraphStore.setState({
      graph: testGraph,
      contextFiles: new Set(),
      sidebarTab: 'overview',
    })

    // 1. Bug Fix Preset adds target file + 1-hop callers
    useGraphStore.getState().applyBugFixPreset('src/auth/service.ts')
    expect(useGraphStore.getState().contextFiles.has('src/auth/service.ts')).toBe(true)
    expect(useGraphStore.getState().contextFiles.has('src/auth/handler.ts')).toBe(true)
    expect(useGraphStore.getState().sidebarTab).toBe('context')

    // 2. Feature Preset adds route endpoints
    useGraphStore.getState().clearContextWorkspace()
    useGraphStore.getState().applyFeaturePreset(0)
    expect(useGraphStore.getState().contextFiles.size).toBeGreaterThan(0)
  })

  it('atomically resets graph state on resetGraphState and prevents stale node leaks', () => {
    const testGraph = graph({
      nodes: [node('src/index.ts')],
    })
    useGraphStore.setState({
      graph: testGraph,
      activeProjectRoot: '/old/project',
      selected: new Set(['src/index.ts']),
      contextFiles: new Set(['src/index.ts']),
      status: 'synced',
    })

    useGraphStore.getState().resetGraphState()
    const state = useGraphStore.getState()
    expect(state.graph).toBeNull()
    expect(state.activeProjectRoot).toBeNull()
    expect(state.selected.size).toBe(0)
    expect(state.contextFiles.size).toBe(0)
    expect(state.status).toBe('stale')
  })

  it('correctly cleans UNC path prefixes', () => {
    expect(cleanUncPath('//?/C:/My-pro/innovexinfo/frontend')).toBe('C:/My-pro/innovexinfo/frontend')
    expect(cleanUncPath('\\\\?\\C:\\My-pro\\innovexinfo\\frontend')).toBe('C:/My-pro/innovexinfo/frontend')
    expect(cleanUncPath('C:/My-pro/innovexinfo/frontend')).toBe('C:/My-pro/innovexinfo/frontend')
    expect(cleanUncPath('')).toBe('')
  })
})



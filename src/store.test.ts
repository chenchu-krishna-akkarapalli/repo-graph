import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { AgentActivity, RepoGraph } from './types'
import { agentActivityTargets, primarySelection, useGraphStore } from './store'

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

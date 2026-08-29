import { create } from 'zustand'
import Fuse from 'fuse.js'
import type { FileTreeNode, GraphNode, RepoGraph, SyncStatus, AgentActivity, GitFileStatus } from './types'
import { loadGraph, tauriInvoke } from './lib/loadGraph'
import { buildTreeFromPaths, cleanUncPath } from './lib/fileTree'
import { applyHoverHighlight } from './lib/hoverHighlight'
import { KNOWN_LANGUAGES } from './lib/layout'
import { savePersistedGraph, loadPersistedGraph } from './lib/graphCache'
import { detectCommunities } from './lib/community'

export type SidebarTab = 'overview' | 'dependencies' | 'impact' | 'callgraph' | 'context'

const EMPTY: ReadonlySet<string> = new Set()

/** Module-level fuzzy index (not reactive state — rebuilt on graph load). */
let fuse: Fuse<GraphNode> | null = null

/** rAF coalescing for hover (see `setHovered`). */
let hoverFrame: number | null = null
let pendingHover: string | null = null

export type IndexPhase = 'idle' | 'walking' | 'parsing' | 'db_write' | 'complete'

export interface IndexProgress {
  phase: IndexPhase
  filesTotal: number
  filesProcessed: number
  bytesTotal: number
  bytesProcessed: number
  /** Exponentially-smoothed files/sec; 0 until the first real sample. */
  speedFilesPerSec: number
  /** Null until a speed estimate exists (or once the run is `complete`, 0). */
  etaSeconds: number | null
}

const IDLE_INDEX_PROGRESS: IndexProgress = {
  phase: 'idle',
  filesTotal: 0,
  filesProcessed: 0,
  bytesTotal: 0,
  bytesProcessed: 0,
  speedFilesPerSec: 0,
  etaSeconds: null,
}

/** Raw `index_progress` Tauri event payload (Rust's `serde` field names). */
export interface RawIndexProgressPayload {
  phase: IndexPhase
  files_total: number
  files_processed: number
  bytes_total: number
  bytes_processed: number
}

/** Blend weight for the EMA over instantaneous files/sec samples — high
 *  enough to track real speed changes, low enough that one noisy poll
 *  (a burst of tiny files, a stall on one huge one) doesn't whiplash the ETA. */
const PROGRESS_EMA_ALPHA = 0.3

/** Timing state for the instantaneous-rate calc in `setIndexProgress`.
 *  Module-level like `hoverFrame` above: derived from event deltas, not
 *  itself something a consumer reads, and reset per indexing run. */
let progressLastTs: number | null = null
let progressLastFiles = 0

function resetIndexProgressTracking() {
  progressLastTs = null
  progressLastFiles = 0
}

/** How long a node stays highlighted after an agent touches it. */
const AGENT_PULSE_MS = 2000
/** Rolling window of agent tool calls kept for the telemetry panel. */
const ACTIVITY_LOG_LIMIT = 100

/** In-flight pulse timers, so they can be cancelled rather than leaked. */
const activityTimers = new Set<ReturnType<typeof setTimeout>>()

/**
 * Node ids an activity should highlight.
 *
 * A `path` identifies the file directly. A bare `symbol` has to be located,
 * which costs a scan of the graph — done once here rather than twice (add and
 * remove) as it was before.
 */
export function agentActivityTargets(
  graph: RepoGraph | null,
  activity: AgentActivity,
): string[] {
  const targets = new Set<string>()

  let rawPath = activity.path?.trim() || ''
  let symbol = activity.symbol?.trim() || null

  // If path contains 'file#symbol' notation
  if (rawPath.includes('#') && !symbol) {
    const parts = rawPath.split('#')
    rawPath = parts[0]
    symbol = parts[1]
  }

  const normPath = rawPath
    .replace(/\\/g, '/')
    .replace(/^\.?\//, '')
    .trim()

  if (normPath) {
    targets.add(normPath)
    if (symbol) {
      targets.add(`${normPath}#${symbol}`)
    }
    // Also check against graph nodes for exact or relative suffix match
    if (graph) {
      for (const node of graph.nodes) {
        if (node.path === normPath || node.path.endsWith(`/${normPath}`) || normPath.endsWith(`/${node.path}`)) {
          targets.add(node.path)
          if (symbol) {
            targets.add(`${node.path}#${symbol}`)
          }
        }
      }
    }
  }

  if (symbol && graph) {
    for (const node of graph.nodes) {
      if (node.symbols.some((s) => s.name === symbol)) {
        targets.add(node.path)
        targets.add(`${node.path}#${symbol}`)
      }
    }
  }

  return Array.from(targets)
}

/**
 * Files in descending centrality — the UI counterpart of the MCP tool's
 * `top_k`.
 *
 * Sorts on `rank_order` (dense, computed once by the backend over the whole
 * graph) rather than re-deriving an order from `rank_score`, so the numbers
 * shown here and the `#N` badges on the canvas can never disagree. Unranked
 * nodes — an older cache, or a test fixture — sort last instead of colonising
 * position 1, which is what `rank_order: 0` would otherwise do.
 */
export function rankedFiles(graph: RepoGraph | null, limit?: number): GraphNode[] {
  if (!graph) return []
  const ranked = graph.nodes
    .filter((n) => (n.rank_order ?? 0) > 0)
    .sort((a, b) => (a.rank_order ?? 0) - (b.rank_order ?? 0))
  return limit === undefined ? ranked : ranked.slice(0, limit)
}

interface GraphState {
  graph: RepoGraph | null
  status: SyncStatus
  indexLatencyMs: number
  loadError: string | null
  /** O(1) adjacency lookups (RULES.md §4: no linear scans per query). */
  dependenciesOf: Map<string, string[]>
  dependentsOf: Map<string, string[]>
  neighborsOf: Map<string, Set<string>>

  selected: ReadonlySet<string>
  hoveredPath: string | null
  searchQuery: string
  searchMatches: ReadonlySet<string>
  searchLatencyMs: number
  languageFilters: ReadonlySet<string>
  collapsedDirs: ReadonlySet<string>
  /** Hide canvas nodes scoring below this centrality. `0` shows everything. */
  minRank: number
  setMinRank: (value: number) => void

  densityMode: 'full' | 'core' | 'domains'
  setDensityMode: (mode: 'full' | 'core' | 'domains') => void
  spotlightMode: boolean
  toggleSpotlightMode: () => void

  impactSource: string | null
  impactSet: ReadonlySet<string>

  sidebarTab: SidebarTab
  sidebarWidth: number
  sidebarOpen: boolean

  gitStatus: ReadonlyMap<string, 'modified' | 'added' | 'deleted' | 'untracked'>
  showGitDiff: boolean
  toggleShowGitDiff: () => void
  refreshGitStatus: () => Promise<void>

  applyBugFixPreset: (filePath: string) => void
  applyRefactorPreset: (domainId: number) => void
  applyFeaturePreset: (domainId: number) => void

  /** Playbook §7 project slice. */
  activeProjectRoot: string | null
  fileTree: FileTreeNode[]
  isIndexing: boolean
  indexProgress: IndexProgress
  setIndexProgress: (payload: RawIndexProgressPayload) => void
  /** One-shot camera request consumed by GraphCanvas (nonce dedupes). */
  focusRequest: { path: string; nonce: number } | null

  readFiles: ReadonlySet<string>
  toggleReadFile: (path: string) => void
  clearReadFiles: () => void

  contextFiles: ReadonlySet<string>
  addFileToContext: (path: string) => void
  removeFileFromContext: (path: string) => void
  clearContextWorkspace: () => void

  contextSymbols: ReadonlySet<string>
  addSymbolToContext: (path: string, symbolName: string) => void
  removeSymbolFromContext: (path: string, symbolName: string) => void
  
  expandedFiles: ReadonlySet<string>
  toggleExpandFile: (path: string) => void
  
  selectedSymbol: { path: string; name: string } | null
  setSelectedSymbol: (symbol: { path: string; name: string } | null) => void

  agentActivity: AgentActivity[]
  activeAgentTargets: Set<string>
  addAgentActivity: (activity: AgentActivity) => void
  /** Drops the log and cancels every in-flight pulse timer. */
  clearAgentActivity: () => void

  load: () => Promise<void>
  resetGraphState: () => void
  openProject: () => Promise<void>
  selectProject: (root: string) => Promise<void>
  focusNode: (path: string) => void
  select: (path: string, additive: boolean) => void
  clearSelection: () => void
  setHovered: (path: string | null) => void
  /** Return to the Project Hub landing dashboard. */
  goHome: () => void
  setSearchQuery: (query: string) => void
  toggleLanguageFilter: (language: string) => void
  toggleDir: (dir: string) => void
  simulateImpact: (path: string) => void
  clearImpact: () => void
  setSidebarTab: (tab: SidebarTab) => void
  setSidebarWidth: (width: number) => void
  toggleSidebar: () => void
}

export const useGraphStore = create<GraphState>((set, get) => ({
  graph: null,
  status: 'indexing',
  indexLatencyMs: 0,
  loadError: null,
  dependenciesOf: new Map(),
  dependentsOf: new Map(),
  neighborsOf: new Map(),
  agentActivity: [],
  activeAgentTargets: new Set(),

  selected: EMPTY,
  hoveredPath: null,
  searchQuery: '',
  searchMatches: EMPTY,
  searchLatencyMs: 0,
  // Every supported language is on by default; a filter that silently hides
  // files would make the map lie about what is in the repo.
  languageFilters: new Set<string>(KNOWN_LANGUAGES),
  collapsedDirs: EMPTY,
  minRank: 0,

  densityMode: 'full',
  setDensityMode: (densityMode) => set({ densityMode }),
  spotlightMode: false,
  toggleSpotlightMode: () => set((s) => ({ spotlightMode: !s.spotlightMode })),

  impactSource: null,
  impactSet: EMPTY,

  sidebarTab: 'overview',
  sidebarWidth: 360,
  sidebarOpen: true,

  gitStatus: new Map(),
  showGitDiff: true,
  toggleShowGitDiff: () => set((s) => ({ showGitDiff: !s.showGitDiff })),

  activeProjectRoot: null,
  fileTree: [],
  isIndexing: false,
  indexProgress: IDLE_INDEX_PROGRESS,
  focusRequest: null,
  readFiles: EMPTY,
  contextFiles: EMPTY,
  contextSymbols: EMPTY,
  expandedFiles: EMPTY,
  selectedSymbol: null,

  load: async () => {
    set({ status: 'indexing', loadError: null })
    try {
      const { graph, latencyMs } = await loadGraph()
      const invoke = tauriInvoke()
      // Cold start lands on the Project Hub: the cached graph is warmed in the
      // background but `activeProjectRoot` stays null until the user picks a
      // project. A `graph_updated` reload keeps whatever project is open.
      const root = get().activeProjectRoot
      let fileTree = buildTreeFromPaths(graph.nodes.map((n) => n.path))
      if (invoke && root) {
        try {
          fileTree = await invoke('read_directory_tree', { root }) as FileTreeNode[]
        } catch {
          // Browser dev build or an unreadable root: the path-derived tree
          // built above is a fine fallback, so this is not worth surfacing.
        }
      }
      savePersistedGraph(root, graph)
      set({
        ...ingestGraph(graph),
        status: 'synced',
        indexLatencyMs: latencyMs,
        activeProjectRoot: root,
        fileTree,
        // Startup scale guard; a `graph_updated` reload keeps whatever the
        // user has already expanded (non-empty set = leave it alone).
        collapsedDirs:
          get().collapsedDirs.size > 0 ? get().collapsedDirs : autoCollapsedDirs(graph),
        contextFiles: EMPTY,
        contextSymbols: EMPTY,
        expandedFiles: EMPTY,
        selectedSymbol: null,
      })
      void get().refreshGitStatus()
    } catch (e) {
      // Auto-Rehydration Layer: If memory graph is missing or load failed,
      // attempt recovery from local persistent cache without crashing.
      const persisted = loadPersistedGraph(get().activeProjectRoot)
      if (persisted && persisted.graph && persisted.graph.nodes.length > 0) {
        console.info('[store] Auto-rehydrated graph from persistent cache.')
        set({
          ...ingestGraph(persisted.graph),
          status: 'synced',
          loadError: null,
          activeProjectRoot: get().activeProjectRoot || persisted.root,
          fileTree: buildTreeFromPaths(persisted.graph.nodes.map((n) => n.path)),
          collapsedDirs:
            get().collapsedDirs.size > 0 ? get().collapsedDirs : autoCollapsedDirs(persisted.graph),
        })
      } else if (!get().graph) {
        fuse = null
        set({ status: 'stale', loadError: e instanceof Error ? e.message : String(e) })
      } else {
        // Cache Eviction Guard: keep existing graph in memory
        set({ status: 'stale', loadError: e instanceof Error ? e.message : String(e) })
      }
    }
  },

  resetGraphState: () => {
    resetIndexProgressTracking()
    fuse = null
    set({
      graph: null,
      status: 'stale',
      fileTree: [],
      activeProjectRoot: null,
      selected: EMPTY,
      hoveredPath: null,
      impactSource: null,
      impactSet: EMPTY,
      contextFiles: EMPTY,
      contextSymbols: EMPTY,
      searchQuery: '',
      searchMatches: EMPTY,
      collapsedDirs: EMPTY,
      readFiles: EMPTY,
      expandedFiles: EMPTY,
      selectedSymbol: null,
      gitStatus: new Map(),
      activeAgentTargets: new Set(),
      dependenciesOf: new Map(),
      dependentsOf: new Map(),
      neighborsOf: new Map(),
      loadError: null,
      isIndexing: false,
      indexProgress: IDLE_INDEX_PROGRESS,
    })
  },

  openProject: async () => {
    const invoke = tauriInvoke()
    if (!invoke) return // browser dev build: native dialog unavailable
    const rawRoot = (await invoke('open_project_dialog')) as string | null
    if (!rawRoot) return // user cancelled
    const root = cleanUncPath(rawRoot)
    resetIndexProgressTracking()
    fuse = null
    // Atomically clear previous graph and node state immediately
    set({
      graph: null,
      fileTree: [],
      activeProjectRoot: root,
      selected: EMPTY,
      hoveredPath: null,
      impactSource: null,
      impactSet: EMPTY,
      contextFiles: EMPTY,
      contextSymbols: EMPTY,
      searchQuery: '',
      searchMatches: EMPTY,
      collapsedDirs: EMPTY,
      readFiles: EMPTY,
      expandedFiles: EMPTY,
      selectedSymbol: null,
      gitStatus: new Map(),
      activeAgentTargets: new Set(),
      dependenciesOf: new Map(),
      dependentsOf: new Map(),
      neighborsOf: new Map(),
      isIndexing: true,
      status: 'indexing',
      loadError: null,
      indexProgress: IDLE_INDEX_PROGRESS,
    })
    try {
      const started = performance.now()
      const graph = (await invoke('index_and_load_graph', { root })) as RepoGraph
      const fileTree = (await invoke('read_directory_tree', { root })) as FileTreeNode[]
      savePersistedGraph(root, graph)
      set({
        ...ingestGraph(graph),
        status: 'synced',
        indexLatencyMs: Math.round(performance.now() - started),
        activeProjectRoot: root,
        fileTree,
        isIndexing: false,
        indexProgress: IDLE_INDEX_PROGRESS,
        selected: EMPTY,
        impactSource: null,
        impactSet: EMPTY,
        searchQuery: '',
        searchMatches: EMPTY,
        collapsedDirs: autoCollapsedDirs(graph),
        readFiles: EMPTY,
        contextFiles: EMPTY,
        contextSymbols: EMPTY,
        expandedFiles: EMPTY,
        selectedSymbol: null,
      })
      void get().refreshGitStatus()
    } catch (e) {
      set({
        isIndexing: false,
        status: 'stale',
        loadError: e instanceof Error ? e.message : String(e),
        indexProgress: IDLE_INDEX_PROGRESS,
      })
    }
  },

  selectProject: async (rawRoot: string) => {
    const invoke = tauriInvoke()
    if (!invoke) return
    const root = cleanUncPath(rawRoot)
    resetIndexProgressTracking()
    fuse = null
    // Atomically clear previous graph and node state immediately
    set({
      graph: null,
      fileTree: [],
      activeProjectRoot: root,
      selected: EMPTY,
      hoveredPath: null,
      impactSource: null,
      impactSet: EMPTY,
      contextFiles: EMPTY,
      contextSymbols: EMPTY,
      searchQuery: '',
      searchMatches: EMPTY,
      collapsedDirs: EMPTY,
      readFiles: EMPTY,
      expandedFiles: EMPTY,
      selectedSymbol: null,
      gitStatus: new Map(),
      activeAgentTargets: new Set(),
      dependenciesOf: new Map(),
      dependentsOf: new Map(),
      neighborsOf: new Map(),
      isIndexing: true,
      status: 'indexing',
      loadError: null,
      indexProgress: IDLE_INDEX_PROGRESS,
    })
    try {
      const started = performance.now()
      const graph = (await invoke('index_and_load_graph', { root })) as RepoGraph
      const fileTree = (await invoke('read_directory_tree', { root })) as FileTreeNode[]
      savePersistedGraph(root, graph)
      set({
        ...ingestGraph(graph),
        status: 'synced',
        indexLatencyMs: Math.round(performance.now() - started),
        activeProjectRoot: root,
        fileTree,
        isIndexing: false,
        indexProgress: IDLE_INDEX_PROGRESS,
        selected: EMPTY,
        impactSource: null,
        impactSet: EMPTY,
        searchQuery: '',
        searchMatches: EMPTY,
        collapsedDirs: autoCollapsedDirs(graph),
        readFiles: EMPTY,
        contextFiles: EMPTY,
        contextSymbols: EMPTY,
        expandedFiles: EMPTY,
        selectedSymbol: null,
      })
      void get().refreshGitStatus()
    } catch (e) {
      set({
        isIndexing: false,
        status: 'stale',
        loadError: e instanceof Error ? e.message : String(e),
        indexProgress: IDLE_INDEX_PROGRESS,
      })
    }
  },

  focusNode: (path) =>
    set((s) => ({
      selected: new Set([path]),
      sidebarOpen: true,
      focusRequest: { path, nonce: (s.focusRequest?.nonce ?? 0) + 1 },
    })),

  select: (path, additive) =>
    set((s) => {
      const next = new Set(additive ? s.selected : [])
      if (additive && next.has(path)) next.delete(path)
      else next.add(path)
      return { selected: next, sidebarOpen: true }
    }),

  clearSelection: () =>
    set({ selected: EMPTY, impactSource: null, impactSet: EMPTY, selectedSymbol: null }),

  /**
   * Hover is applied to the DOM immediately (one CSSOM rule swap, O(1)) and
   * mirrored into the store at most once per animation frame. No canvas
   * component subscribes to `hoveredPath`, so a fast mouse sweep over a
   * 1,000-node graph costs zero React renders.
   */
  setHovered: (path) => {
    applyHoverHighlight(path)
    pendingHover = path
    if (hoverFrame !== null) return
    hoverFrame = requestAnimationFrame(() => {
      hoverFrame = null
      if (get().hoveredPath !== pendingHover) set({ hoveredPath: pendingHover })
    })
  },

  goHome: () => {
    applyHoverHighlight(null)
    set({
      activeProjectRoot: null,
      hoveredPath: null,
      selected: EMPTY,
      selectedSymbol: null,
      impactSource: null,
      impactSet: EMPTY,
      searchQuery: '',
      searchMatches: EMPTY,
    })
  },

  setSearchQuery: (query) => {
    if (!query.trim() || !fuse) {
      set({ searchQuery: query, searchMatches: EMPTY, searchLatencyMs: 0 })
      return
    }
    const started = performance.now()
    const matches = new Set(fuse.search(query, { limit: 50 }).map((r) => r.item.path))
    set({
      searchQuery: query,
      searchMatches: matches,
      searchLatencyMs: Math.round((performance.now() - started) * 10) / 10,
    })
  },

  toggleLanguageFilter: (language) =>
    set((s) => {
      const next = new Set(s.languageFilters)
      if (next.has(language)) next.delete(language)
      else next.add(language)
      return { languageFilters: next }
    }),

  // Clamped rather than trusted: this drives what the canvas shows, and a
  // value outside [0, 1] would silently blank it.
  setMinRank: (value) => set({ minRank: Math.min(1, Math.max(0, value)) }),

  toggleDir: (dir) =>
    set((s) => {
      const next = new Set(s.collapsedDirs)
      if (next.has(dir)) next.delete(dir)
      else next.add(dir)
      return { collapsedDirs: next }
    }),

  simulateImpact: (path) => {
    // Forward walk over dependents = every file that (transitively) breaks.
    const { dependentsOf } = get()
    const affected = new Set<string>()
    const queue = [path]
    while (queue.length > 0) {
      const current = queue.pop()!
      for (const dependent of dependentsOf.get(current) ?? []) {
        if (!affected.has(dependent)) {
          affected.add(dependent)
          queue.push(dependent)
        }
      }
    }
    set({ impactSource: path, impactSet: affected, sidebarTab: 'impact' })
  },

  clearImpact: () => set({ impactSource: null, impactSet: EMPTY }),

  toggleReadFile: (path) =>
    set((s) => {
      const next = new Set(s.readFiles)
      if (next.has(path)) next.delete(path)
      else next.add(path)
      return { readFiles: next }
    }),
  clearReadFiles: () => set({ readFiles: EMPTY }),

  addFileToContext: (path) =>
    set((s) => {
      const next = new Set(s.contextFiles)
      next.add(path)
      return { contextFiles: next }
    }),
  removeFileFromContext: (path) =>
    set((s) => {
      const next = new Set(s.contextFiles)
      next.delete(path)
      return { contextFiles: next }
    }),
  clearContextWorkspace: () => set({ contextFiles: EMPTY, contextSymbols: EMPTY }),

  refreshGitStatus: async () => {
    const root = get().activeProjectRoot
    const invoke = tauriInvoke()
    if (!invoke || !root) return
    try {
      const res = (await invoke('get_git_status', { root })) as GitFileStatus[]
      const map = new Map<string, 'modified' | 'added' | 'deleted' | 'untracked'>()
      for (const item of res) {
        map.set(item.path, item.status)
      }
      set({ gitStatus: map })
    } catch {
      // Ignored if git is unavailable
    }
  },

  applyBugFixPreset: (filePath: string) => {
    const graph = get().graph
    const nextFiles = new Set(get().contextFiles)
    nextFiles.add(filePath)
    if (graph) {
      for (const edge of graph.edges) {
        if (edge.to_path === filePath) {
          nextFiles.add(edge.from_path)
        }
      }
    }
    set({ contextFiles: nextFiles, sidebarTab: 'context', sidebarOpen: true })
  },

  applyRefactorPreset: (domainId: number) => {
    const graph = get().graph
    if (!graph) return
    const { communities } = detectCommunities(graph)
    const domain = communities.find((d) => d.id === domainId)
    const nextFiles = new Set(get().contextFiles)
    if (domain) {
      for (const f of domain.nodes) {
        nextFiles.add(f)
      }
      const { dependentsOf } = get()
      for (const f of domain.nodes) {
        const queue = [f]
        while (queue.length > 0) {
          const curr = queue.pop()!
          for (const dep of dependentsOf.get(curr) ?? []) {
            if (!nextFiles.has(dep)) {
              nextFiles.add(dep)
              queue.push(dep)
            }
          }
        }
      }
    }
    set({ contextFiles: nextFiles, sidebarTab: 'context', sidebarOpen: true })
  },

  applyFeaturePreset: (domainId: number) => {
    const graph = get().graph
    if (!graph) return
    const { communities } = detectCommunities(graph)
    const domain = communities.find((d) => d.id === domainId)
    const nextFiles = new Set(get().contextFiles)
    if (domain) {
      const domainNodeSet = new Set(domain.nodes)
      for (const node of graph.nodes) {
        if (domainNodeSet.has(node.path)) {
          if (node.routes.length > 0 || (node.rank_score ?? 0) >= 0.5) {
            nextFiles.add(node.path)
          }
        }
      }
      if (nextFiles.size === 0) {
        for (const f of domain.nodes.slice(0, 5)) {
          nextFiles.add(f)
        }
      }
    }
    set({ contextFiles: nextFiles, sidebarTab: 'context', sidebarOpen: true })
  },

  addSymbolToContext: (path, symbolName) =>
    set((s) => {
      const next = new Set(s.contextSymbols)
      next.add(`${path}#${symbolName}`)
      return { contextSymbols: next }
    }),
  removeSymbolFromContext: (path, symbolName) =>
    set((s) => {
      const next = new Set(s.contextSymbols)
      next.delete(`${path}#${symbolName}`)
      return { contextSymbols: next }
    }),

  toggleExpandFile: (path) =>
    set((s) => {
      const next = new Set(s.expandedFiles)
      if (next.has(path)) next.delete(path)
      else next.add(path)
      return { expandedFiles: next }
    }),

  setSelectedSymbol: (symbol) => set({ selectedSymbol: symbol }),

  /**
   * Records one agent tool call and pulses the nodes it touched.
   *
   * The timer is scheduled *outside* the `set()` updater. A Zustand updater
   * must be pure — scheduling from inside it leaked one untracked 2 s timeout
   * per activity with no way to cancel them, and React's StrictMode double
   * invocation doubled the count.
   */
  addAgentActivity: (activity) => {
    const targets = agentActivityTargets(get().graph, activity)

    set((state) => {
      const nextTargets = new Set(state.activeAgentTargets)
      for (const t of targets) nextTargets.add(t)
      return {
        agentActivity: [activity, ...state.agentActivity].slice(0, ACTIVITY_LOG_LIMIT),
        activeAgentTargets: nextTargets,
      }
    })

    if (targets.length === 0) return

    const handle = setTimeout(() => {
      activityTimers.delete(handle)
      set((state) => {
        const nextTargets = new Set(state.activeAgentTargets)
        for (const t of targets) nextTargets.delete(t)
        return { activeAgentTargets: nextTargets }
      })
    }, AGENT_PULSE_MS)
    activityTimers.add(handle)
  },

  clearAgentActivity: () => {
    for (const handle of activityTimers) clearTimeout(handle)
    activityTimers.clear()
    set({ agentActivity: [], activeAgentTargets: new Set() })
  },

  /**
   * Consumes one `index_progress` Tauri event. Speed is an EMA over the
   * instantaneous files/sec since the previous event (not a cumulative
   * average from indexing start), so a mid-run slowdown on a large file
   * shows up in the ETA within a poll or two instead of being diluted by
   * every fast file that came before it.
   */
  setIndexProgress: (payload) => {
    const now = performance.now()
    const filesProcessed = payload.files_processed
    const filesTotal = payload.files_total

    let instRate = 0
    if (progressLastTs !== null) {
      const dtSec = (now - progressLastTs) / 1000
      const dFiles = filesProcessed - progressLastFiles
      if (dtSec > 0 && dFiles > 0) instRate = dFiles / dtSec
    }
    progressLastTs = now
    progressLastFiles = filesProcessed

    set((s) => {
      const prevSpeed = s.indexProgress.speedFilesPerSec
      const speedFilesPerSec =
        instRate <= 0 ? prevSpeed
        : prevSpeed <= 0 ? instRate
        : PROGRESS_EMA_ALPHA * instRate + (1 - PROGRESS_EMA_ALPHA) * prevSpeed

      const remaining = filesTotal - filesProcessed
      const etaSeconds =
        payload.phase === 'complete' ? 0
        : speedFilesPerSec > 0 && remaining > 0 ? remaining / speedFilesPerSec
        : null

      return {
        indexProgress: {
          phase: payload.phase,
          filesTotal,
          filesProcessed,
          bytesTotal: payload.bytes_total,
          bytesProcessed: payload.bytes_processed,
          speedFilesPerSec,
          etaSeconds,
        },
      }
    })
  },

  setSidebarTab: (tab) => set({ sidebarTab: tab }),
  setSidebarWidth: (width) =>
    set({ sidebarWidth: Math.min(440, Math.max(320, width)) }),
  toggleSidebar: () => set((s) => ({ sidebarOpen: !s.sidebarOpen })),
}))

// Dev-only handle for perf harnesses (frame-time probes, synthetic graphs).
// Stripped from production builds by the `import.meta.env.DEV` guard.
if (import.meta.env.DEV) {
  ;(window as unknown as Record<string, unknown>).__REPO_GRAPH_STORE__ = useGraphStore
}

/**
 * Scale guard: past this node count the canvas starts fully collapsed.
 * 10,000+ file/symbol DOM elements will freeze or crash the web view, so the
 * initial render is a high-level folder map the user drills into instead.
 */
const AUTO_COLLAPSE_NODE_LIMIT = 1000

/**
 * Every ancestor directory of every node when the graph is over the limit
 * (outermost collapsed ancestor wins in `buildFlow`, so this renders one
 * folder node per top-level directory), or the empty set for small graphs.
 */
function autoCollapsedDirs(graph: RepoGraph): ReadonlySet<string> {
  if (graph.nodes.length <= AUTO_COLLAPSE_NODE_LIMIT) return EMPTY
  const dirs = new Set<string>()
  for (const node of graph.nodes) {
    const parts = node.path.split('/')
    for (let i = 1; i < parts.length; i++) {
      dirs.add(parts.slice(0, i).join('/'))
    }
  }
  return dirs
}

/** Build adjacency indexes + fuzzy index for a freshly loaded graph. */
function ingestGraph(graph: RepoGraph) {
  const dependenciesOf = new Map<string, string[]>()
  const dependentsOf = new Map<string, string[]>()
  const neighborsOf = new Map<string, Set<string>>()
  for (const edge of graph.edges) {
    push(dependenciesOf, edge.from_path, edge.to_path)
    push(dependentsOf, edge.to_path, edge.from_path)
    pushSet(neighborsOf, edge.from_path, edge.to_path)
    pushSet(neighborsOf, edge.to_path, edge.from_path)
  }
  fuse = new Fuse(graph.nodes, {
    keys: ['path', 'exports', 'routes'],
    threshold: 0.35,
    ignoreLocation: true,
  })
  return { graph, dependenciesOf, dependentsOf, neighborsOf }
}

function push(map: Map<string, string[]>, key: string, value: string) {
  const list = map.get(key)
  if (list) list.push(value)
  else map.set(key, [value])
}

function pushSet(map: Map<string, Set<string>>, key: string, value: string) {
  const bag = map.get(key)
  if (bag) bag.add(value)
  else map.set(key, new Set([value]))
}

/** Primary selection shown in the Detail Sidebar (last selected wins). */
export function primarySelection(selected: ReadonlySet<string>): string | null {
  let last: string | null = null
  for (const path of selected) last = path
  return last
}

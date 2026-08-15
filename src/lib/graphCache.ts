import type { RepoGraph } from '../types'

const CACHE_ROOT_KEY = 'repograph:cached_graph_root'
const CACHE_DATA_KEY = 'repograph:cached_graph_data'
const CACHE_TIME_KEY = 'repograph:cached_graph_timestamp'

export interface PersistedGraphCache {
  root: string | null
  graph: RepoGraph
  timestamp: number
}

/**
 * Persist graph structure and active root to local storage with quota safety.
 */
export function savePersistedGraph(root: string | null, graph: RepoGraph): boolean {
  try {
    if (!graph || !Array.isArray(graph.nodes)) return false
    const serialized = JSON.stringify(graph)
    localStorage.setItem(CACHE_DATA_KEY, serialized)
    if (root) {
      localStorage.setItem(CACHE_ROOT_KEY, root)
    } else {
      localStorage.removeItem(CACHE_ROOT_KEY)
    }
    localStorage.setItem(CACHE_TIME_KEY, Date.now().toString())
    return true
  } catch (err) {
    console.warn('[graphCache] Failed to persist graph to localStorage:', err)
    return false
  }
}

/**
 * Retrieve persisted graph cache from local storage if valid.
 */
export function loadPersistedGraph(expectedRoot?: string | null): PersistedGraphCache | null {
  try {
    const raw = localStorage.getItem(CACHE_DATA_KEY)
    if (!raw) return null
    const graph = JSON.parse(raw) as RepoGraph
    if (!graph || !Array.isArray(graph.nodes) || !Array.isArray(graph.edges)) {
      return null
    }
    const root = localStorage.getItem(CACHE_ROOT_KEY)
    const timeStr = localStorage.getItem(CACHE_TIME_KEY)
    const timestamp = timeStr ? parseInt(timeStr, 10) : 0

    if (expectedRoot !== undefined && expectedRoot !== null && root && root !== expectedRoot) {
      // Root mismatch: different workspace requested
      return null
    }

    return { root, graph, timestamp }
  } catch (err) {
    console.warn('[graphCache] Failed to load persisted graph from localStorage:', err)
    return null
  }
}

/**
 * Cache Eviction Guard:
 * Purge cache only on explicit workspace switch or user request.
 */
export function clearPersistedGraph(): void {
  try {
    localStorage.removeItem(CACHE_DATA_KEY)
    localStorage.removeItem(CACHE_ROOT_KEY)
    localStorage.removeItem(CACHE_TIME_KEY)
  } catch (err) {
    console.warn('[graphCache] Failed to clear persisted graph:', err)
  }
}

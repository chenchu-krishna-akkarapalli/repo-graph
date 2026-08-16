import type { RepoGraph, GraphNode } from '../types'

/**
 * Deterministic community detection & graph clustering using modularity optimization.
 * Groups tightly coupled files and symbols into cohesive architectural domains.
 */

export interface CommunityResult {
  /** Map from node path to community ID (0, 1, 2, ...) */
  nodeCommunityMap: Map<string, number>
  /** Summary of each detected community */
  communities: CommunitySummary[]
}

export interface CommunitySummary {
  id: number
  name: string
  nodeCount: number
  nodes: string[]
  topHub: string
  cohesionScore: number // 0.0 - 1.0
  colorClass: string
  bgClass: string
  borderClass: string
}

export const COMMUNITY_PALETTE = [
  { color: '#38BDF8', colorClass: 'text-sky-400', bgClass: 'bg-sky-500/15', borderClass: 'border-sky-500/40' },
  { color: '#34D399', colorClass: 'text-emerald-400', bgClass: 'bg-emerald-500/15', borderClass: 'border-emerald-500/40' },
  { color: '#FBBF24', colorClass: 'text-amber-400', bgClass: 'bg-amber-500/15', borderClass: 'border-amber-500/40' },
  { color: '#F472B6', colorClass: 'text-pink-400', bgClass: 'bg-pink-500/15', borderClass: 'border-pink-500/40' },
  { color: '#818CF8', colorClass: 'text-indigo-400', bgClass: 'bg-indigo-500/15', borderClass: 'border-indigo-500/40' },
  { color: '#A78BFA', colorClass: 'text-purple-400', bgClass: 'bg-purple-500/15', borderClass: 'border-purple-500/40' },
  { color: '#FB923C', colorClass: 'text-orange-400', bgClass: 'bg-orange-500/15', borderClass: 'border-orange-500/40' },
  { color: '#2DD4BF', colorClass: 'text-teal-400', bgClass: 'bg-teal-500/15', borderClass: 'border-teal-500/40' },
  { color: '#E879F9', colorClass: 'text-fuchsia-400', bgClass: 'bg-fuchsia-500/15', borderClass: 'border-fuchsia-500/40' },
  { color: '#94A3B8', colorClass: 'text-slate-400', bgClass: 'bg-slate-500/15', borderClass: 'border-slate-500/40' },
]

/**
 * Detects communities in the given dependency graph.
 */
export function detectCommunities(graph: RepoGraph | null): CommunityResult {
  if (!graph || graph.nodes.length === 0) {
    return { nodeCommunityMap: new Map(), communities: [] }
  }

  const nodes: GraphNode[] = graph.nodes
  const edges = graph.edges

  // Build undirected adjacency list for modularity calculation
  const adjacency = new Map<string, Set<string>>()
  for (const n of nodes) {
    adjacency.set(n.path, new Set())
  }

  for (const e of edges) {
    if (e.from_path !== e.to_path) {
      adjacency.get(e.from_path)?.add(e.to_path)
      adjacency.get(e.to_path)?.add(e.from_path)
    }
  }

  // Initial step: each node in its own community
  const communityOf = new Map<string, number>()
  nodes.forEach((n: GraphNode, idx: number) => communityOf.set(n.path, idx))

  // Greedy modularity optimization passes
  let improved = true
  let passes = 0
  const maxPasses = 8

  while (improved && passes < maxPasses) {
    improved = false
    passes++

    for (const node of nodes) {
      const currentComm = communityOf.get(node.path)!
      const neighbors = Array.from(adjacency.get(node.path) || [])
      if (neighbors.length === 0) continue

      // Count neighbor community frequencies
      const commVotes = new Map<number, number>()
      for (const neighbor of neighbors) {
        const nComm = communityOf.get(neighbor)!
        commVotes.set(nComm, (commVotes.get(nComm) || 0) + 1)
      }

      // Find community with highest neighbor affinity
      let bestComm = currentComm
      let maxVotes = commVotes.get(currentComm) || 0

      for (const [candComm, votes] of commVotes.entries()) {
        if (votes > maxVotes) {
          maxVotes = votes
          bestComm = candComm
        }
      }

      if (bestComm !== currentComm) {
        communityOf.set(node.path, bestComm)
        improved = true
      }
    }
  }

  // Renumber and group communities
  const commGroups = new Map<number, string[]>()
  for (const [path, commId] of communityOf.entries()) {
    const list = commGroups.get(commId) || []
    list.push(path)
    commGroups.set(commId, list)
  }

  // Sort communities by size descending
  const sortedCommEntries = Array.from(commGroups.values()).sort(
    (a, b) => b.length - a.length
  )

  const nodeCommunityMap = new Map<string, number>()
  const nodeMap = new Map<string, GraphNode>(nodes.map((n: GraphNode) => [n.path, n]))
  const communities: CommunitySummary[] = []

  sortedCommEntries.forEach((members, newId) => {
    for (const path of members) {
      nodeCommunityMap.set(path, newId)
    }

    // Find top hub node in this community
    let topHub = members[0]
    let maxRank = -1
    for (const path of members) {
      const node = nodeMap.get(path)
      const rank = node?.rank_score ?? 0
      if (rank > maxRank) {
        maxRank = rank
        topHub = path
      }
    }

    // Calculate cohesion score (internal edges / possible internal edges)
    const memberSet = new Set(members)
    let internalEdges = 0
    for (const path of members) {
      const neighbors = adjacency.get(path) || new Set()
      for (const n of neighbors) {
        if (memberSet.has(n)) {
          internalEdges++
        }
      }
    }
    // Each undirected edge counted twice
    const actualEdges = internalEdges / 2
    const possibleEdges = (members.length * (members.length - 1)) / 2
    const cohesionScore =
      possibleEdges > 0 ? Math.min(1.0, actualEdges / possibleEdges) : 1.0

    // Auto-derive community name from directory path common prefix or top hub
    const name = deriveCommunityName(members, topHub)
    const palette = COMMUNITY_PALETTE[newId % COMMUNITY_PALETTE.length]

    communities.push({
      id: newId,
      name,
      nodeCount: members.length,
      nodes: members,
      topHub,
      cohesionScore,
      colorClass: palette.colorClass,
      bgClass: palette.bgClass,
      borderClass: palette.borderClass,
    })
  })

  return { nodeCommunityMap, communities }
}

/**
 * Derives a human-readable domain name for a community from common directory prefixes or hub name.
 */
function deriveCommunityName(members: string[], topHub: string): string {
  if (members.length === 1) {
    return members[0].split('/').pop() || members[0]
  }

  // Check common folder prefix
  const dirCounts = new Map<string, number>()
  for (const m of members) {
    const parts = m.split('/')
    if (parts.length > 1) {
      const dir = parts.slice(0, 2).join('/')
      dirCounts.set(dir, (dirCounts.get(dir) || 0) + 1)
    }
  }

  let bestDir = ''
  let maxDirCount = 0
  for (const [dir, count] of dirCounts.entries()) {
    if (count > maxDirCount) {
      maxDirCount = count
      bestDir = dir
    }
  }

  if (bestDir && maxDirCount >= members.length * 0.45) {
    return bestDir
  }

  const hubName = topHub.split('/').pop()?.split('.')[0] || topHub
  return `${hubName} Domain`
}

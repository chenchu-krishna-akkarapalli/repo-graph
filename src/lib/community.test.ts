import { describe, it, expect } from 'vitest'
import { detectCommunities } from './community'
import type { RepoGraph } from '../types'

describe('detectCommunities', () => {
  it('handles empty and null graph gracefully', () => {
    expect(detectCommunities(null).communities).toEqual([])
    expect(
      detectCommunities({
        schema_version: 1,
        nodes: [],
        edges: [],
        external_dependencies: [],
        warnings: [],
      }).communities
    ).toEqual([])
  })

  it('clusters tightly connected subgraphs into distinct communities', () => {
    const mockGraph: RepoGraph = {
      schema_version: 1,
      nodes: [
        // Cluster A
        {
          path: 'src/auth/login.ts',
          language: 'typescript',
          size_bytes: 500,
          exports: [],
          routes: [],
          in_degree: 1,
          out_degree: 2,
          symbols: [],
        },
        {
          path: 'src/auth/session.ts',
          language: 'typescript',
          size_bytes: 400,
          exports: [],
          routes: [],
          in_degree: 1,
          out_degree: 1,
          symbols: [],
        },
        {
          path: 'src/auth/token.ts',
          language: 'typescript',
          size_bytes: 300,
          exports: [],
          routes: [],
          in_degree: 2,
          out_degree: 0,
          symbols: [],
        },
        // Cluster B
        {
          path: 'src-tauri/engine/parser.rs',
          language: 'rust',
          size_bytes: 1200,
          exports: [],
          routes: [],
          in_degree: 1,
          out_degree: 1,
          symbols: [],
        },
        {
          path: 'src-tauri/engine/graph.rs',
          language: 'rust',
          size_bytes: 1500,
          exports: [],
          routes: [],
          in_degree: 2,
          out_degree: 0,
          symbols: [],
        },
        {
          path: 'src-tauri/engine/mcp.rs',
          language: 'rust',
          size_bytes: 800,
          exports: [],
          routes: [],
          in_degree: 0,
          out_degree: 2,
          symbols: [],
        },
      ],
      edges: [
        // Intra Cluster A edges
        { from_path: 'src/auth/login.ts', to_path: 'src/auth/session.ts', kind: 'imports' },
        { from_path: 'src/auth/session.ts', to_path: 'src/auth/token.ts', kind: 'imports' },
        { from_path: 'src/auth/login.ts', to_path: 'src/auth/token.ts', kind: 'imports' },
        // Intra Cluster B edges
        { from_path: 'src-tauri/engine/parser.rs', to_path: 'src-tauri/engine/graph.rs', kind: 'imports' },
        { from_path: 'src-tauri/engine/mcp.rs', to_path: 'src-tauri/engine/graph.rs', kind: 'imports' },
        { from_path: 'src-tauri/engine/mcp.rs', to_path: 'src-tauri/engine/parser.rs', kind: 'imports' },
      ],
      external_dependencies: [],
      warnings: [],
    }

    const result = detectCommunities(mockGraph)
    expect(result.communities.length).toBeGreaterThanOrEqual(2)

    // Verify nodes in same cluster share the same community ID
    const loginComm = result.nodeCommunityMap.get('src/auth/login.ts')
    const sessionComm = result.nodeCommunityMap.get('src/auth/session.ts')
    const parserComm = result.nodeCommunityMap.get('src-tauri/engine/parser.rs')
    const graphComm = result.nodeCommunityMap.get('src-tauri/engine/graph.rs')

    expect(loginComm).toBe(sessionComm)
    expect(parserComm).toBe(graphComm)
    expect(loginComm).not.toBe(parserComm)
  })
})

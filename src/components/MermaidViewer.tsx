import { useMemo, useState } from 'react'
import { Copy, Check, Code } from 'lucide-react'
import { useGraphStore } from '../store'
import type { RepoGraph, GraphEdge, GraphNode } from '../types'

interface MermaidViewerProps {
  rootPath?: string
  direction?: 'TD' | 'LR'
}

/**
 * Generates Mermaid diagram code from a graph subgraph centered around rootPath.
 */
export function generateMermaidCode(
  graph: RepoGraph | null,
  rootPath?: string,
  direction: 'TD' | 'LR' = 'TD'
): string {
  if (!graph || graph.nodes.length === 0) {
    return 'graph TD\n  empty["No graph data"]'
  }

  const safeId = (p: string) =>
    p.replace(/[^a-zA-Z0-9_]/g, '_').replace(/^_+|_+$/g, '') || 'root'

  let targetEdges: GraphEdge[] = graph.edges
  let targetNodes: GraphNode[] = graph.nodes

  if (rootPath) {
    // 1-to-2 hop neighborhood around rootPath
    const relevantPaths = new Set<string>([rootPath])

    for (const e of graph.edges) {
      if (e.from_path === rootPath || e.to_path === rootPath) {
        relevantPaths.add(e.from_path)
        relevantPaths.add(e.to_path)
      }
    }

    targetEdges = graph.edges.filter(
      (e: GraphEdge) => relevantPaths.has(e.from_path) && relevantPaths.has(e.to_path)
    )
    targetNodes = graph.nodes.filter((n: GraphNode) => relevantPaths.has(n.path))
  }

  let code = `flowchart ${direction}\n`

  // Node declarations with friendly labels
  for (const node of targetNodes.slice(0, 25)) {
    const id = safeId(node.path)
    const label = node.path.split('/').pop() || node.path
    const isRoot = node.path === rootPath
    if (isRoot) {
      code += `  ${id}["⭐ ${label}"]:::rootNode\n`
    } else {
      code += `  ${id}["${label}"]\n`
    }
  }

  // Edge declarations
  for (const edge of targetEdges.slice(0, 35)) {
    const fromId = safeId(edge.from_path)
    const toId = safeId(edge.to_path)
    if (fromId && toId && fromId !== toId) {
      if (edge.kind === 'route') {
        code += `  ${fromId} -.->|route| ${toId}\n`
      } else {
        code += `  ${fromId} --> ${toId}\n`
      }
    }
  }

  code += `\n  classDef rootNode fill:#2563EB20,stroke:#3B82F6,stroke-width:2px,color:#fff;`

  return code
}

export default function MermaidViewer({
  rootPath,
  direction = 'TD',
}: MermaidViewerProps) {
  const graph = useGraphStore((s) => s.graph)
  const focusNode = useGraphStore((s) => s.focusNode)
  const [copied, setCopied] = useState(false)
  const [showRaw, setShowRaw] = useState(false)

  const mermaidCode = useMemo(
    () => generateMermaidCode(graph, rootPath, direction),
    [graph, rootPath, direction]
  )

  const handleCopy = async () => {
    try {
      if (navigator.clipboard) {
        await navigator.clipboard.writeText(mermaidCode)
        setCopied(true)
        setTimeout(() => setCopied(false), 2000)
      }
    } catch {
      // Fallback
    }
  }

  return (
    <div className="flex flex-col rounded-xl border border-white/10 bg-[#080B10]/80 overflow-hidden">
      {/* Header controls */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-white/10 bg-white/[0.02]">
        <span className="text-[11px] font-mono font-semibold text-white/70 flex items-center gap-1.5">
          <Code size={13} className="text-blue-400" />
          Mermaid Architecture Flow
        </span>
        <div className="flex items-center gap-1">
          <button
            onClick={() => setShowRaw(!showRaw)}
            className="px-2 py-0.5 rounded text-[10px] font-mono text-white/50 hover:text-white hover:bg-white/10 transition-colors border-0 bg-transparent cursor-pointer"
          >
            {showRaw ? 'Visual Tree' : 'Code'}
          </button>
          <button
            onClick={() => void handleCopy()}
            className="flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-mono text-white/60 hover:text-white hover:bg-white/10 transition-colors border-0 bg-transparent cursor-pointer"
            title="Copy Mermaid Code"
          >
            {copied ? <Check size={11} className="text-emerald-400" /> : <Copy size={11} />}
            {copied ? 'Copied' : 'Copy'}
          </button>
        </div>
      </div>

      {/* Visual / Code Container */}
      <div className="p-3">
        {showRaw ? (
          <pre className="max-h-48 overflow-y-auto font-mono text-[10px] text-blue-200 bg-black/40 p-2.5 rounded-lg border border-white/5 select-all scrollbar-thin">
            {mermaidCode}
          </pre>
        ) : (
          <div className="flex flex-col gap-2 max-h-56 overflow-y-auto scrollbar-thin">
            <div className="text-[10px] text-white/40 mb-1">
              Topological Call Graph Flow:
            </div>
            <div className="space-y-1 font-mono text-[11px]">
              {graph?.edges
                .filter((e) => !rootPath || e.from_path === rootPath || e.to_path === rootPath)
                .slice(0, 10)
                .map((e, idx) => (
                  <div
                    key={`${e.from_path}-${e.to_path}-${idx}`}
                    className="flex items-center justify-between p-1.5 rounded-lg bg-white/[0.02] border border-white/5 hover:bg-white/[0.05] transition-colors"
                  >
                    <button
                      onClick={() => focusNode(e.from_path)}
                      className="text-white/80 hover:text-blue-300 truncate max-w-[130px] border-0 bg-transparent cursor-pointer text-left text-[10px]"
                      title={e.from_path}
                    >
                      {e.from_path.split('/').pop()}
                    </button>
                    <span className="text-[9px] text-white/30 px-1">──►</span>
                    <button
                      onClick={() => focusNode(e.to_path)}
                      className="text-white/80 hover:text-blue-300 truncate max-w-[130px] border-0 bg-transparent cursor-pointer text-left text-[10px]"
                      title={e.to_path}
                    >
                      {e.to_path.split('/').pop()}
                    </button>
                  </div>
                ))}
            </div>
          </div>
        )}
      </div>
    </div>
  )
}

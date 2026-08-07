import { memo, useState } from 'react'
import { BaseEdge, getBezierPath, type EdgeProps } from 'reactflow'
import { useGraphStore } from '../../store'
import type { RepoGraph } from '../../types'

/** Kind of the symbol behind a `path#name` node id (null for file nodes). */
function symbolKindOf(graph: RepoGraph | null, id: string): string | null {
  const idx = id.indexOf('#')
  if (idx === -1 || !graph) return null
  const file = id.slice(0, idx)
  const name = id.slice(idx + 1)
  return (
    graph.nodes.find((n) => n.path === file)?.symbols?.find((sy) => sy.name === name)?.kind ??
    null
  )
}

/**
 * Dependency edge with reactive highlight states:
 * - hover on a node: outgoing dependencies yellow, incoming dependents red
 * - impact simulation: every edge on the downstream path flashes red
 * - provenance: heuristic connections styled as dashed pink lines with hover annotations
 */
function EdgeHighlight(props: EdgeProps) {
  const { id, source, target, data } = props
  const [path, labelX, labelY] = getBezierPath(props)
  const [isHovered, setIsHovered] = useState(false)

  const { provenance, wiring_site, isSymbolEdge } = data || {}

  const isImpactEdge = useGraphStore((s) => {
    if (s.impactSource === null) return false
    const inScope = (p: string) => p === s.impactSource || s.impactSet.has(p)
    const srcFile = source.split('#')[0]
    const tgtFile = target.split('#')[0]
    return inScope(srcFile) && inScope(tgtFile)
  })

  // Hover-driven dim/highlight is applied by the CSS rules keyed off the
  // `data-edge-source` / `data-edge-target` attributes below, so a mouse
  // sweep never re-renders the edge layer.
  const sourceFile = source.split('#')[0]
  const targetFile = target.split('#')[0]

  // §20.2 semantic edge styling by endpoint symbol kinds.
  const sourceKind = useGraphStore((s) =>
    isSymbolEdge ? symbolKindOf(s.graph, source) : null,
  )
  const targetKind = useGraphStore((s) =>
    isSymbolEdge ? symbolKindOf(s.graph, target) : null,
  )
  const isRouteBinding = sourceKind === 'route' || targetKind === 'route'
  const isStateUsage =
    !isRouteBinding &&
    (targetKind === 'state_store' || sourceKind === 'state_store' || wiring_site === 'state_usage')
  const isComponentChild =
    !isRouteBinding &&
    !isStateUsage &&
    (targetKind === 'component' || wiring_site === 'JSX Component Child')

  let stroke = isSymbolEdge ? '#8B5CF6' : '#2A3447'
  let width = isSymbolEdge ? 1.5 : 1.2
  let strokeDasharray: string | undefined = undefined

  if (provenance === 'heuristic') {
    stroke = '#EC4899' // pink-500
    strokeDasharray = '4'
  }
  if (isRouteBinding) {
    // API route → handler: solid emerald.
    stroke = '#10B981' // emerald-500
    width = 1.8
    strokeDasharray = undefined
  } else if (isStateUsage) {
    // Shared state read/write: dotted gold, matching the amber store nodes.
    stroke = '#F59E0B' // amber-500
    width = 1.6
    strokeDasharray = '2 3'
  } else if (isComponentChild) {
    // Component nesting: dashed cyan.
    stroke = '#06B6D4' // cyan-500
    strokeDasharray = '5 3'
  }

  if (isImpactEdge) {
    stroke = 'var(--color-impact-red)'
    width = 2
  }

  return (
    <g
      className={isImpactEdge ? 'impact-edge graph-edge' : 'graph-edge'}
      data-edge-source={sourceFile}
      data-edge-target={targetFile}
      onMouseEnter={() => setIsHovered(true)}
      onMouseLeave={() => setIsHovered(false)}
    >
      <path
        d={path}
        fill="none"
        stroke="transparent"
        strokeWidth={12}
        className="cursor-pointer"
      />
      <BaseEdge
        id={id}
        path={path}
        style={{
          stroke,
          strokeWidth: width,
          strokeDasharray,
          transition: 'stroke 150ms',
        }}
      />
      {isHovered && isRouteBinding && (
        <foreignObject
          x={labelX - 110}
          y={labelY - 40}
          width={220}
          height={65}
          className="pointer-events-none"
        >
          <div className="flex flex-col gap-0.5 rounded-lg border border-white/10 bg-[#0F1218]/95 px-2.5 py-2 text-[10px] font-mono text-emerald-300 shadow-xl backdrop-blur-md">
            <div className="text-center font-semibold text-emerald-400">API Endpoint Binding</div>
            <div className="truncate text-white/70 text-center">
              {(sourceKind === 'route' ? source : target).split('#')[1]}
              <span className="text-emerald-400"> → </span>
              {(sourceKind === 'route' ? target : source).split('#')[1]}
            </div>
          </div>
        </foreignObject>
      )}
      {isHovered && isStateUsage && (
        <foreignObject
          x={labelX - 110}
          y={labelY - 40}
          width={220}
          height={65}
          className="pointer-events-none"
        >
          <div className="flex flex-col gap-0.5 rounded-lg border border-amber-500/30 bg-[#0F1218]/95 px-2.5 py-2 text-[10px] font-mono text-amber-300 shadow-xl backdrop-blur-md">
            <div className="text-center font-semibold text-amber-400">State Store Usage</div>
            <div className="truncate text-center text-white/70">
              {(sourceKind === 'state_store' ? target : source).split('#')[1]}
              <span className="text-amber-400"> → </span>
              {(sourceKind === 'state_store' ? source : target).split('#')[1]}
            </div>
          </div>
        </foreignObject>
      )}
      {isHovered && !isRouteBinding && !isStateUsage && provenance === 'heuristic' && (
        <foreignObject
          x={labelX - 100}
          y={labelY - 45}
          width={200}
          height={80}
          className="pointer-events-none"
        >
          <div className="flex flex-col gap-0.5 rounded-lg border border-white/10 bg-[#0F1218]/95 px-2.5 py-2 text-[10px] text-pink-300 font-mono shadow-xl backdrop-blur-md">
            <div className="font-semibold text-pink-400 text-center">Heuristic Connection</div>
            <div className="text-white/70 text-center">Wiring: <span className="text-pink-300 font-semibold">{wiring_site || 'useContext'}</span></div>
          </div>
        </foreignObject>
      )}
    </g>
  )
}

export default memo(EdgeHighlight)

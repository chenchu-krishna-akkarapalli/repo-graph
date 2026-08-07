import { memo } from 'react'
import { Handle, Position, type NodeProps } from 'reactflow'
import { FileCode2, Route, ChevronDown, ChevronRight } from 'lucide-react'
import { useShallow } from 'zustand/react/shallow'
import { useGraphStore } from '../../store'
import type { FileNodeData } from '../../lib/layout'

const LANGUAGE_CLASSES: Record<string, string> = {
  javascript: 'bg-[#0F2038]/85 border-[#3B82F6]/60 shadow-[0_0_12px_rgba(59,130,246,0.15)]',
  python: 'bg-[#0F2C1A]/85 border-[#10B981]/60 shadow-[0_0_12px_rgba(16,185,129,0.15)]',
  rust: 'bg-[#2C180B]/85 border-[#F97316]/60 shadow-[0_0_12px_rgba(249,115,22,0.15)]',
  other: 'bg-[#11151E]/90 border-white/10',
}

function CustomFileNode({ id, data, selected }: NodeProps<FileNodeData>) {
  // One shallow-compared selector instead of six subscriptions: a store
  // update touches this component once, not six times, per node.
  // Hover/dim state is deliberately absent — it is driven entirely by the
  // CSS rules in `hoverHighlight.ts` via the data attributes below, so mouse
  // movement never re-renders any node.
  const { isImpact, isImpactSource, isSearchMatch, isSearchMiss, isAgentTarget } = useGraphStore(
    useShallow((s) => {
      const searching = s.searchQuery.trim() !== ''
      return {
        isImpact: s.impactSet.has(id),
        isImpactSource: s.impactSource === id,
        isSearchMatch: searching && s.searchMatches.has(id),
        isSearchMiss: searching && !s.searchMatches.has(id),
        isAgentTarget: s.activeAgentTargets.has(id),
      }
    }),
  )

  const toggleExpandFile = useGraphStore((s) => s.toggleExpandFile)
  const isExpanded = data.isExpanded
  const hasSymbols = data.symbols && data.symbols.length > 0

  const handleToggleExpand = (e: React.MouseEvent) => {
    e.stopPropagation()
    toggleExpandFile(id)
  }

  const border = selected
    ? '!border-violet-500 shadow-[0_0_16px_rgba(139,92,246,0.5)] ring-1 ring-violet-500/50'
    : isImpact || isImpactSource
      ? '!border-rose-500 shadow-[0_0_16px_rgba(244,63,94,0.5)]'
      : isSearchMatch
        ? '!border-amber-400 shadow-[0_0_14px_rgba(251,191,36,0.5)]'
        : ''

  return (
    <div
      data-node-path={id}
      data-node-neighbors={data.neighbors}
      className={[
        'graph-node h-full rounded-xl border px-3 py-1.5 shadow-xl backdrop-blur-md hover:border-violet-500/40 hover:shadow-violet-500/10',
        LANGUAGE_CLASSES[data.language] ?? LANGUAGE_CLASSES.other,
        border,
        isImpact || isImpactSource ? 'impact-node' : '',
        isAgentTarget ? 'agent-radar-node' : '',
        isSearchMiss ? 'opacity-25' : '',
      ].join(' ')}
      title={data.path}
    >
      <Handle type="target" position={Position.Right} className="!h-2.5 !w-2.5 !border-0 !bg-white/30" />
      <div className="flex w-full items-center gap-1.5 text-[13px] font-medium text-white/90">
        <FileCode2 size={13} className="shrink-0 opacity-70" />
        <span className="truncate max-w-[100px]">{data.name}</span>
        {data.routes.length > 0 && <Route size={12} className="shrink-0 text-amber-400" />}
        {hasSymbols && (
          <button
            onClick={handleToggleExpand}
            className="ml-auto rounded-md p-0.5 hover:bg-white/15 text-white/50 hover:text-white transition-all border-0 bg-transparent cursor-pointer"
            title={isExpanded ? "Collapse symbols" : "Expand symbols"}
          >
            {isExpanded ? (
              <ChevronDown size={12} className="transition-transform duration-200" />
            ) : (
              <ChevronRight size={12} className="transition-transform duration-200" />
            )}
          </button>
        )}
      </div>
      <div className="truncate font-mono text-[10px] tracking-tight text-white/40">
        {data.routes[0] ?? (data.dir || '/')}
      </div>
      <Handle type="source" position={Position.Left} className="!h-2.5 !w-2.5 !border-0 !bg-white/30" />
    </div>
  )
}

export default memo(CustomFileNode)

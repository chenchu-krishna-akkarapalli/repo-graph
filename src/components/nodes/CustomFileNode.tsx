import { memo } from 'react'
import { Handle, Position, type NodeProps } from 'reactflow'
import { FileCode2, Route, ChevronDown, ChevronRight } from 'lucide-react'
import { useShallow } from 'zustand/react/shallow'
import { useGraphStore } from '../../store'
import type { FileNodeData } from '../../lib/layout'

const CORE_RANK = 0.5

const LANGUAGE_CLASSES: Record<string, string> = {
  javascript: 'bg-[#0E1726]/90 border-blue-500/40 hover:border-blue-400',
  python: 'bg-[#0B1E16]/90 border-emerald-500/40 hover:border-emerald-400',
  rust: 'bg-[#22130B]/90 border-orange-500/40 hover:border-orange-400',
  other: 'bg-[#10141D]/90 border-white/10 hover:border-white/25',
}

function CustomFileNode({ id, data, selected }: NodeProps<FileNodeData>) {
  const { isImpact, isImpactSource, isSearchMatch, isSearchMiss, isAgentTarget, isSpotlightDimmed } = useGraphStore(
    useShallow((s) => {
      const searching = s.searchQuery.trim() !== ''
      let isSpotlightDimmed = false
      if (s.spotlightMode && s.selected.size > 0) {
        let isNeighborOrSelected = s.selected.has(id)
        if (!isNeighborOrSelected) {
          for (const sel of s.selected) {
            const neighbors = s.neighborsOf.get(sel)
            if (neighbors && neighbors.has(id)) {
              isNeighborOrSelected = true
              break
            }
          }
        }
        isSpotlightDimmed = !isNeighborOrSelected
      }
      return {
        isImpact: s.impactSet.has(id),
        isImpactSource: s.impactSource === id,
        isSearchMatch: searching && s.searchMatches.has(id),
        isSearchMiss: searching && !s.searchMatches.has(id),
        isAgentTarget: s.activeAgentTargets.has(id),
        isSpotlightDimmed,
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

  const isCore = data.rank >= CORE_RANK
  const gitStatus = useGraphStore((s) => (s.showGitDiff ? s.gitStatus.get(id) : undefined))

  const stateBorder = selected
    ? '!border-blue-400 shadow-[0_0_14px_rgba(59,130,246,0.4)] ring-1 ring-blue-400/40'
    : isImpact || isImpactSource
      ? '!border-rose-500 shadow-[0_0_14px_rgba(244,63,94,0.4)]'
      : isSearchMatch
        ? '!border-amber-400 shadow-[0_0_12px_rgba(251,191,36,0.4)]'
        : gitStatus
          ? '!border-amber-500/60 shadow-[0_0_12px_rgba(245,158,11,0.25)]'
          : ''

  return (
    <div
      data-node-path={id}
      data-node-neighbors={data.neighbors}
      className={[
        'graph-node h-full rounded-xl border px-3 py-1.5 shadow-lg backdrop-blur-md transition-all duration-200',
        LANGUAGE_CLASSES[data.language] ?? LANGUAGE_CLASSES.other,
        stateBorder,
        isImpact || isImpactSource ? 'impact-node' : '',
        isAgentTarget ? 'agent-radar-node' : '',
        isSearchMiss ? 'opacity-25' : '',
        isSpotlightDimmed ? 'opacity-15 grayscale-[60%] scale-[0.98]' : '',
      ].join(' ')}
      title={
        data.rankOrder > 0
          ? `${data.path}\nRank #${data.rankOrder} · centrality ${data.rank.toFixed(3)} · ${data.dependents} dependents`
          : data.path
      }
    >
      <Handle type="target" position={Position.Right} className="!h-2 !w-2 !border-0 !bg-white/40" />
      <div className="flex w-full items-center gap-1.5 text-xs font-semibold text-white/95">
        <FileCode2 size={13} className="shrink-0 opacity-70" />
        <span className="truncate max-w-[100px]">{data.name}</span>
        {gitStatus && (
          <span
            className={`px-1 py-0.2 rounded text-[9px] font-bold border font-mono ${
              gitStatus === 'modified'
                ? 'bg-amber-500/20 text-amber-300 border-amber-500/40'
                : gitStatus === 'added'
                  ? 'bg-emerald-500/20 text-emerald-300 border-emerald-500/40'
                  : 'bg-cyan-500/20 text-cyan-300 border-cyan-500/40'
            }`}
            title={`Git: ${gitStatus}`}
          >
            {gitStatus === 'modified' ? 'M' : gitStatus === 'added' ? 'A' : '?'}
          </span>
        )}
        {data.routes.length > 0 && <Route size={12} className="shrink-0 text-amber-400" />}
        {data.domainName && (
          <span
            className={`ml-auto text-[9px] px-1 py-0.2 rounded border font-mono truncate max-w-[70px] ${data.domainColor ?? 'text-sky-300'} ${data.domainBg ?? 'bg-sky-500/10'} ${data.domainBorder ?? 'border-sky-500/20'}`}
            title={`Domain: ${data.domainName}`}
          >
            {data.domainName}
          </span>
        )}
        {hasSymbols && (
          <button
            onClick={handleToggleExpand}
            className="ml-auto rounded p-0.5 hover:bg-white/15 text-white/40 hover:text-white transition-all border-0 bg-transparent cursor-pointer"
            title={isExpanded ? 'Collapse symbols' : 'Expand symbols'}
          >
            {isExpanded ? (
              <ChevronDown size={12} className="transition-transform duration-150" />
            ) : (
              <ChevronRight size={12} className="transition-transform duration-150" />
            )}
          </button>
        )}
      </div>
      <div className="flex items-center gap-1.5 font-mono text-[10px] tracking-tight text-white/40 mt-0.5">
        <span className="truncate">{data.routes[0] ?? (data.dir || '/')}</span>
        {data.rankOrder > 0 && (
          <span
            className={[
              'ml-auto shrink-0 rounded px-1 text-[9px] leading-[13px] font-mono tabular-nums',
              isCore ? 'bg-blue-500/20 text-blue-300 font-bold' : 'bg-white/[0.06] text-white/40',
            ].join(' ')}
            title={`Rank #${data.rankOrder} · ${data.dependents} dependents`}
          >
            #{data.rankOrder}
          </span>
        )}
      </div>
      <Handle type="source" position={Position.Left} className="!h-2 !w-2 !border-0 !bg-white/40" />
    </div>
  )
}

export default memo(CustomFileNode)

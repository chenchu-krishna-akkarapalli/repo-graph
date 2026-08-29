import { memo, type MouseEvent } from 'react'
import { Handle, Position, type NodeProps } from 'reactflow'
import { Plus, Check, Link2 } from 'lucide-react'
import { useGraphStore } from '../../store'
import { httpMethodOf, METHOD_BADGE } from '../../lib/httpMethod'

interface SymbolNodeData {
  name: string
  kind: string
  parentFile: string
  /** Parent file's adjacency string — symbols dim/light with their file. */
  neighbors: string
}

/** Semantic node themes: emerald API routes, cyan components,
 *  purple default — tuned against the #090A0F canvas. */
const KIND_THEMES = {
  route: {
    container: 'border-emerald-500/30 bg-[#0F241B]/60 text-emerald-300 hover:border-emerald-400/80 shadow-md shadow-black/40 backdrop-blur-xl',
    handle: '!bg-emerald-400',
  },
  component: {
    container: 'border-cyan-500/30 bg-[#0F2028]/60 text-cyan-300 hover:border-cyan-400/80 shadow-md shadow-black/40 backdrop-blur-xl',
    handle: '!bg-cyan-400',
  },
  /** Zustand/Redux/Context/Pinia/Vuex/MobX containers — amber reads as
   *  "shared mutable state", distinct from routes and components. */
  state_store: {
    container: 'border-amber-500/30 bg-[#261d10]/60 text-amber-300 hover:border-amber-400 shadow-md shadow-black/40 backdrop-blur-xl',
    handle: '!bg-amber-400',
  },
  /** ORM models / table declarations — indigo reads as "persisted data",
   *  a different axis from runtime state (amber) or transport (emerald). */
  database_schema: {
    container: 'border-indigo-500/30 bg-[#17162e]/60 text-indigo-300 hover:border-indigo-400 shadow-md shadow-black/40 backdrop-blur-xl',
    handle: '!bg-indigo-400',
  },
  /** Pub/sub topics and emitter events — rose signals "fires elsewhere",
   *  which is what makes these edges hard to follow by reading code. */
  event_channel: {
    container: 'border-rose-500/30 bg-[#281519]/60 text-rose-300 hover:border-rose-400 shadow-md shadow-black/40 backdrop-blur-xl',
    handle: '!bg-rose-400',
  },
  default: {
    container: 'border-purple-500/30 bg-[#1A1528]/60 text-purple-300 hover:border-purple-400/80 shadow-md shadow-black/40 backdrop-blur-xl',
    handle: '!bg-violet-400',
  },
} as const

function themeFor(kind: string) {
  if (kind === 'route') return KIND_THEMES.route
  if (kind === 'component') return KIND_THEMES.component
  if (kind === 'state_store') return KIND_THEMES.state_store
  if (kind === 'database_schema') return KIND_THEMES.database_schema
  if (kind === 'event_channel') return KIND_THEMES.event_channel
  return KIND_THEMES.default
}

function CustomSymbolNode({ id, data }: NodeProps<SymbolNodeData>) {
  const contextSymbols = useGraphStore((s) => s.contextSymbols)
  const addSymbolToContext = useGraphStore((s) => s.addSymbolToContext)
  const removeSymbolFromContext = useGraphStore((s) => s.removeSymbolFromContext)
  const setSelectedSymbol = useGraphStore((s) => s.setSelectedSymbol)
  const setSidebarTab = useGraphStore((s) => s.setSidebarTab)
  const focusNode = useGraphStore((s) => s.focusNode)

  const selectedSymbol = useGraphStore((s) => s.selectedSymbol)
  const isSelected = selectedSymbol?.path === data.parentFile && selectedSymbol?.name === data.name
  const isAgentTarget = useGraphStore((s) => s.activeAgentTargets.has(id) || s.activeAgentTargets.has(`${data.parentFile}#${data.name}`))
  const isCurated = contextSymbols.has(id)

  const handleToggleCuration = (e: MouseEvent) => {
    e.stopPropagation()
    if (isCurated) {
      removeSymbolFromContext(data.parentFile, data.name)
    } else {
      addSymbolToContext(data.parentFile, data.name)
    }
  }

  const handleTraceCallGraph = (e: MouseEvent) => {
    e.stopPropagation()
    focusNode(data.parentFile)
    setSelectedSymbol({ path: data.parentFile, name: data.name })
    setSidebarTab('callgraph')
  }

  const theme = themeFor(data.kind)
  const isRoute = data.kind === 'route'
  const method = isRoute ? httpMethodOf(data.name) : null

  return (
    <div
      data-node-path={data.parentFile}
      data-node-neighbors={data.neighbors}
      className={[
        'graph-node flex h-[34px] w-full items-center justify-between rounded-lg border px-2.5 text-[11px] font-mono backdrop-blur-md',
        isSelected
          ? '!border-blue-400 bg-[#141C2B] text-white shadow-[0_0_10px_rgba(59,130,246,0.3)] ring-1 ring-blue-400/40'
          : theme.container,
        isAgentTarget ? 'agent-radar-node' : '',
      ].join(' ')}
      title={`${data.parentFile}#${data.name}`}
    >
      <Handle
        type="target"
        position={Position.Right}
        className={`!h-1.5 !w-1.5 !border-0 ${theme.handle}`}
      />

      <div className="flex min-w-0 flex-1 items-center gap-1.5">
        {isRoute && method ? (
          <span
            className={`shrink-0 rounded border px-1 text-[8px] font-bold leading-3 tracking-wide ${METHOD_BADGE[method] ?? METHOD_BADGE.GET}`}
            title={`HTTP ${method} endpoint`}
          >
            {method}
          </span>
        ) : data.kind === 'component' ? (
          <span className="shrink-0 text-[9px] font-semibold uppercase text-cyan-400" title="component">
            CMP
          </span>
        ) : data.kind === 'state_store' ? (
          <span
            className="shrink-0 rounded border border-amber-500/40 bg-amber-500/10 px-1 text-[8px] font-bold uppercase leading-3 tracking-wide text-amber-300"
            title="state store"
          >
            STR
          </span>
        ) : data.kind === 'database_schema' ? (
          <span
            className="shrink-0 rounded border border-indigo-500/40 bg-indigo-500/10 px-1 text-[8px] font-bold uppercase leading-3 tracking-wide text-indigo-300"
            title="database model / table"
          >
            DB
          </span>
        ) : data.kind === 'event_channel' ? (
          <span
            className="shrink-0 rounded border border-rose-500/40 bg-rose-500/10 px-1 text-[8px] font-bold uppercase leading-3 tracking-wide text-rose-300"
            title="event / pub-sub channel"
          >
            EVT
          </span>
        ) : (
          <span className="shrink-0 text-[9px] font-semibold uppercase text-violet-400" title={data.kind}>
            {data.kind.substring(0, 3)}
          </span>
        )}
        <span
          className={`truncate ${
            isRoute
              ? 'text-emerald-200'
              : data.kind === 'state_store'
                ? 'text-amber-200'
                : data.kind === 'database_schema'
                  ? 'text-indigo-200'
                  : data.kind === 'event_channel'
                    ? 'text-rose-200'
                    : 'text-white/90'
          }`}
        >
          {data.name}
        </span>
      </div>

      <div className="flex items-center gap-1 shrink-0">
        <button
          onClick={handleTraceCallGraph}
          className="rounded p-0.5 text-white/40 hover:bg-white/10 hover:text-white/90 border-0 bg-transparent cursor-pointer transition-colors"
          title="Trace call graph"
        >
          <Link2 size={10} />
        </button>
        <button
          onClick={handleToggleCuration}
          className={[
            'rounded p-0.5 transition-colors border-0 cursor-pointer',
            isCurated ? 'bg-emerald-500/20 text-emerald-300 hover:bg-emerald-500/30' : 'text-white/40 hover:bg-white/10 hover:text-white/90'
          ].join(' ')}
          title={isCurated ? 'Remove from prompt context' : 'Add to prompt context'}
        >
          {isCurated ? <Check size={10} /> : <Plus size={10} />}
        </button>
      </div>

      <Handle
        type="source"
        position={Position.Left}
        className={`!h-1.5 !w-1.5 !border-0 ${theme.handle}`}
      />
    </div>
  )
}

export default memo(CustomSymbolNode)

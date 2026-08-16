import { useCallback, useMemo, useRef, useState, useEffect, type ReactNode } from 'react'
import {
  AlertTriangle,
  ChevronDown,
  ChevronRight,
  ExternalLink,
  PanelRightClose,
  PanelRightOpen,
  Zap,
  Plus,
  Check,
  ArrowDownRight,
  ArrowUpRight,
  MousePointerClick,
} from 'lucide-react'
import { primarySelection, useGraphStore, type SidebarTab } from '../store'
import { tauriInvoke } from '../lib/loadGraph'
import { detectCommunities } from '../lib/community'
import MermaidViewer from './MermaidViewer'
import type { CallGraph } from '../types'

const TABS: { key: SidebarTab; label: string }[] = [
  { key: 'overview', label: 'Overview' },
  { key: 'dependencies', label: 'Dependencies' },
  { key: 'impact', label: 'Impact' },
  { key: 'callgraph', label: 'Call Graph' },
]

/**
 * Semantic kinds share their canvas badge here — a truncated `kind` rendered
 * "DAT" and "EVE", which read as different concepts from the canvas's DB/EVT.
 * Colours match `CustomSymbolNode.tsx` so the same symbol looks the same in
 * both surfaces. Unlisted kinds keep the generic 3-letter truncation.
 */
const KIND_BADGES: Record<string, { label: string; className: string }> = {
  database_schema: {
    label: 'DB',
    className: 'border border-indigo-500/40 bg-[#17162e] text-indigo-300',
  },
  event_channel: {
    label: 'EVT',
    className: 'border border-rose-500/40 bg-[#281519] text-rose-300',
  },
  state_store: {
    label: 'STR',
    className: 'border border-amber-500/40 bg-[#261d10] text-amber-300',
  },
  route: {
    label: 'API',
    className: 'border border-emerald-500/40 bg-[#0F241B] text-emerald-300',
  },
  component: {
    label: 'CMP',
    className: 'border border-cyan-500/40 bg-[#0F2028] text-cyan-300',
  },
}

/** Badge for a symbol kind, falling back to the generic truncation. */
function KindBadge({ kind, muted = false }: { kind: string; muted?: boolean }) {
  const badge = KIND_BADGES[kind]
  if (!badge) {
    return (
      <span
        className={`text-[9px] font-semibold uppercase mr-1.5 ${muted ? 'text-white/35' : 'text-violet-400'}`}
      >
        {kind.substring(0, 3)}
      </span>
    )
  }
  return (
    <span
      className={`mr-1.5 rounded px-1 text-[8px] font-bold uppercase leading-3 tracking-wide ${badge.className}`}
      title={kind.replace('_', ' ')}
    >
      {badge.label}
    </span>
  )
}

export default function DetailSidebar() {
  const open = useGraphStore((s) => s.sidebarOpen)
  const width = useGraphStore((s) => s.sidebarWidth)
  const setWidth = useGraphStore((s) => s.setSidebarWidth)
  const toggle = useGraphStore((s) => s.toggleSidebar)
  const path = useGraphStore((s) => primarySelection(s.selected))

  // Resize by dragging the left boundary (clamped 320–440 in the store).
  const dragging = useRef(false)
  const onPointerDown = useCallback(
    (e: React.PointerEvent) => {
      dragging.current = true
      e.currentTarget.setPointerCapture(e.pointerId)
    },
    [],
  )
  const onPointerMove = useCallback(
    (e: React.PointerEvent) => {
      if (dragging.current) setWidth(window.innerWidth - e.clientX)
    },
    [setWidth],
  )
  const onPointerUp = useCallback(() => {
    dragging.current = false
  }, [])

  if (!open) {
    return (
      <button
        onClick={toggle}
        className="flex w-9 shrink-0 items-start justify-center border-l border-white/10 bg-[#0D1016] pt-3.5 text-white/40 hover:text-white/90 transition-colors border-0 cursor-pointer"
        title="Open detail panel"
      >
        <PanelRightOpen size={16} />
      </button>
    )
  }

  return (
    <aside
      style={{ width }}
      className="relative flex shrink-0 flex-col border-l border-white/10 bg-[#0D1016]/95 backdrop-blur-md z-30"
    >
      <div
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        className="absolute inset-y-0 -left-1 z-10 w-2 cursor-col-resize hover:bg-violet-500/30 transition-colors"
      />
      <div className="flex items-center justify-between border-b border-white/10 pr-1 bg-black/20">
        <TabBar />
        <button
          onClick={toggle}
          className="rounded-lg p-2 text-white/40 hover:bg-white/[0.06] hover:text-white/90 transition-colors border-0 bg-transparent cursor-pointer ml-1"
          title="Collapse panel"
        >
          <PanelRightClose size={15} />
        </button>
      </div>
      {path ? (
        <SelectedFilePanel path={path} />
      ) : (
        <div className="flex flex-1 flex-col items-center justify-center p-6 text-center">
          <div className="mb-4 flex h-14 w-14 items-center justify-center rounded-2xl border border-white/10 bg-gradient-to-b from-violet-500/15 to-transparent shadow-[0_0_24px_rgba(139,92,246,0.2)] backdrop-blur-md">
            <MousePointerClick size={24} className="text-violet-400" />
          </div>
          <h3 className="mb-1 text-sm font-semibold text-white/90">No Node Selected</h3>
          <p className="max-w-[220px] text-xs text-white/40 leading-relaxed">
            Click any file or symbol on the canvas to inspect its call graph and blast radius.
          </p>
        </div>
      )}
    </aside>
  )
}

function TabBar() {
  const tab = useGraphStore((s) => s.sidebarTab)
  const setTab = useGraphStore((s) => s.setSidebarTab)
  return (
    <nav className="flex flex-1 items-center p-1 gap-1 overflow-x-auto">
      {TABS.map((t) => (
        <button
          key={t.key}
          onClick={() => setTab(t.key)}
          className={[
            'h-7 px-2.5 rounded-md text-[11px] font-semibold transition-all cursor-pointer border-0 whitespace-nowrap',
            tab === t.key
              ? 'bg-white/10 text-white shadow-sm border border-white/10'
              : 'text-white/40 hover:text-white/80 hover:bg-white/[0.04]',
          ].join(' ')}
        >
          {t.label}
        </button>
      ))}
    </nav>
  )
}

function SelectedFilePanel({ path }: { path: string }) {
  const tab = useGraphStore((s) => s.sidebarTab)
  const graph = useGraphStore((s) => s.graph)
  const node = graph?.nodes.find((n) => n.path === path)

  const community = useMemo(() => {
    if (!graph) return null
    const res = detectCommunities(graph)
    const commId = res.nodeCommunityMap.get(path)
    return commId !== undefined ? res.communities.find((c) => c.id === commId) : null
  }, [graph, path])

  if (!node) return null
  return (
    <div className="min-h-0 flex-1 overflow-y-auto p-4 space-y-3">
      {/* File Header Metric Card */}
      <div className="rounded-xl border border-white/10 bg-[#090A0F]/60 p-3 shadow-inner backdrop-blur-sm">
        <div className="mb-1 truncate font-mono text-[13px] font-medium text-white/90" title={node.path}>
          /{node.path}
        </div>
        <div className="flex flex-wrap items-center gap-2 text-xs text-white/40 font-mono">
          <span className="capitalize text-blue-300">{node.language}</span>
          <span>·</span>
          <span>{(node.size_bytes / 1024).toFixed(1)} KB</span>
          <span>·</span>
          <span className="text-white/60">{node.in_degree} in / {node.out_degree} out</span>
          {community && (
            <span className={`ml-auto px-2 py-0.5 rounded-full text-[9px] font-semibold border ${community.colorClass} ${community.bgClass} ${community.borderClass}`}>
              {community.name}
            </span>
          )}
        </div>
      </div>

      {tab === 'overview' && <OverviewTab path={path} />}
      {tab === 'dependencies' && <DependenciesTab path={path} />}
      {tab === 'impact' && <ImpactTab path={path} />}
      {tab === 'callgraph' && <CallGraphTab />}
    </div>
  )
}

/** Progressive disclosure: long symbol lists live behind accordions. */
function Accordion({
  title,
  count,
  defaultOpen,
  children,
}: {
  title: string
  count: number
  defaultOpen?: boolean
  children: ReactNode
}) {
  const [open, setOpen] = useState(defaultOpen ?? false)
  return (
    <div className="mb-2 overflow-hidden rounded-xl border border-white/10 bg-[#090A0F]/40 backdrop-blur-sm transition-all">
      <button
        onClick={() => setOpen((o) => !o)}
        className="flex h-9 w-full items-center gap-1.5 border-0 bg-white/[0.02] px-3 text-xs font-semibold text-white/70 hover:bg-white/[0.06] hover:text-white/95 cursor-pointer transition-colors"
        aria-expanded={open}
      >
        {open ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
        {title}
        <span className="ml-auto font-mono text-[10px] text-white/35 rounded-full bg-white/5 px-2 py-0.5">{count}</span>
      </button>
      {open && <div className="max-h-56 overflow-y-auto p-2.5 space-y-1">{children}</div>}
    </div>
  )
}

function OverviewTab({ path }: { path: string }) {
  const node = useGraphStore((s) => s.graph!.nodes.find((n) => n.path === path))!
  const contextSymbols = useGraphStore((s) => s.contextSymbols)
  const addSymbolToContext = useGraphStore((s) => s.addSymbolToContext)
  const removeSymbolFromContext = useGraphStore((s) => s.removeSymbolFromContext)
  const setSelectedSymbol = useGraphStore((s) => s.setSelectedSymbol)
  const setSidebarTab = useGraphStore((s) => s.setSidebarTab)

  const [exportFilter, setExportFilter] = useState('')
  const shown = node.exports.filter((e) =>
    e.toLowerCase().includes(exportFilter.toLowerCase()),
  )

  const symbols = node.symbols || []
  const [symbolFilter, setSymbolFilter] = useState('')
  const shownSymbols = symbols.filter((s) =>
    s.name.toLowerCase().includes(symbolFilter.toLowerCase()),
  )

  return (
    <>
      {node.routes.length > 0 && (
        <div className="mb-2 rounded-xl border border-amber-500/30 bg-amber-500/10 px-3 py-2 font-mono text-xs text-amber-300 flex items-center gap-2 shadow-sm">
          <span className="font-bold">Route:</span> {node.routes.join(', ')}
        </div>
      )}
      <Accordion title="Exports" count={node.exports.length} defaultOpen={node.exports.length <= 10}>
        {node.exports.length > 10 && (
          <input
            value={exportFilter}
            onChange={(e) => setExportFilter(e.target.value)}
            placeholder="Filter exports…"
            className="mb-2 h-7 w-full rounded-lg border border-white/10 bg-[#090A0F] px-2.5 text-xs text-white/80 focus:border-violet-500 focus:outline-none"
          />
        )}
        {shown.length === 0 ? (
          <Empty label="No exports" />
        ) : (
          shown.map((e) => (
            <div key={e} className="truncate px-2 py-1 font-mono text-xs text-white/70 rounded hover:bg-white/[0.04]">
              {e}
            </div>
          ))
        )}
      </Accordion>

      <Accordion title="Symbols" count={symbols.length} defaultOpen>
        {symbols.length > 10 && (
          <input
            value={symbolFilter}
            onChange={(e) => setSymbolFilter(e.target.value)}
            placeholder="Filter symbols…"
            className="mb-2 h-7 w-full rounded-lg border border-white/10 bg-[#090A0F] px-2.5 text-xs text-white/80 focus:border-violet-500 focus:outline-none"
          />
        )}
        {shownSymbols.length === 0 ? (
          <Empty label="No symbols extracted" />
        ) : (
          <div className="space-y-1">
            {shownSymbols.map((sym) => {
              const refId = `${path}#${sym.name}`
              const isCurated = contextSymbols.has(refId)
              return (
                <div key={sym.name} className="flex items-center justify-between px-2 py-1 rounded-lg hover:bg-white/[0.04] group transition-colors">
                  <div className="truncate flex-1 min-w-0 mr-2">
                    <KindBadge kind={sym.kind} />
                    <span className="font-mono text-xs text-white/80 select-all">{sym.name}</span>
                    <span className="text-[10px] text-white/30 ml-2">L{sym.start_line}-{sym.end_line}</span>
                  </div>
                  <div className="flex items-center gap-1 shrink-0 opacity-40 group-hover:opacity-100 transition-opacity">
                    <button
                      onClick={() => {
                        setSelectedSymbol({ path, name: sym.name })
                        setSidebarTab('callgraph')
                      }}
                      className="rounded p-1 hover:bg-white/10 text-white/60 hover:text-white/95 border-0 bg-transparent cursor-pointer"
                      title="Trace call graph"
                    >
                      <Zap size={11} className="text-violet-400" />
                    </button>
                    <button
                      onClick={() => {
                        if (isCurated) {
                          removeSymbolFromContext(path, sym.name)
                        } else {
                          addSymbolToContext(path, sym.name)
                        }
                      }}
                      className={[
                        'rounded p-1 transition-colors border-0 cursor-pointer',
                        isCurated ? 'bg-emerald-500/20 text-emerald-300 hover:bg-emerald-500/30' : 'hover:bg-white/10 text-white/60 hover:text-white/95 bg-transparent'
                      ].join(' ')}
                      title={isCurated ? 'Remove from prompt context' : 'Add to prompt context'}
                    >
                      {isCurated ? <Check size={11} /> : <Plus size={11} />}
                    </button>
                  </div>
                </div>
              )
            })}
          </div>
        )}
      </Accordion>

      <div className="my-3">
        <MermaidViewer rootPath={path} />
      </div>

      <ActionRow path={path} />
    </>
  )
}

function PathList({ paths, hoverHighlights }: { paths: string[]; hoverHighlights?: boolean }) {
  const setHovered = useGraphStore((s) => s.setHovered)
  const select = useGraphStore((s) => s.select)
  if (paths.length === 0) return <Empty label="None" />
  return (
    <>
      {paths.map((p) => (
        <button
          key={p}
          onClick={() => select(p, false)}
          onMouseEnter={hoverHighlights ? () => setHovered(p) : undefined}
          onMouseLeave={hoverHighlights ? () => setHovered(null) : undefined}
          className="block w-full truncate rounded-lg px-2 py-1 text-left font-mono text-xs text-white/70 hover:bg-white/[0.06] hover:text-white/95 border-0 bg-transparent cursor-pointer transition-colors"
          title={p}
        >
          /{p}
        </button>
      ))}
    </>
  )
}

function DependenciesTab({ path }: { path: string }) {
  const dependencies = useGraphStore((s) => s.dependenciesOf.get(path)) ?? []
  const dependents = useGraphStore((s) => s.dependentsOf.get(path)) ?? []
  return (
    <>
      <Accordion title="Imports (this file depends on)" count={dependencies.length} defaultOpen>
        <PathList paths={dependencies} hoverHighlights />
      </Accordion>
      <Accordion title="Imported by" count={dependents.length}>
        <PathList paths={dependents} hoverHighlights />
      </Accordion>
      <ActionRow path={path} />
    </>
  )
}

function ImpactTab({ path }: { path: string }) {
  const impactSource = useGraphStore((s) => s.impactSource)
  const impactSet = useGraphStore((s) => s.impactSet)
  const simulateImpact = useGraphStore((s) => s.simulateImpact)
  const clearImpact = useGraphStore((s) => s.clearImpact)
  const active = impactSource === path
  const affected = useMemo(() => [...impactSet].sort(), [impactSet])
  return (
    <>
      {active ? (
        <div className="mb-2 flex items-center gap-2 rounded-xl border border-rose-500/40 bg-rose-500/10 px-3 py-2.5 text-xs text-rose-300 shadow-sm">
          <AlertTriangle size={14} className="shrink-0 text-rose-400" />
          Modifying this file affects {impactSet.size} file{impactSet.size === 1 ? '' : 's'}
        </div>
      ) : (
        <p className="mb-2 text-xs text-white/40 leading-relaxed">
          Walk the graph forward to see every file that transitively depends on this one.
        </p>
      )}
      {active && (
        <Accordion title="Affected files" count={affected.length} defaultOpen>
          <PathList paths={affected} />
        </Accordion>
      )}
      <div className="mt-3 flex gap-2 rounded-xl border border-white/10 p-2 bg-[#090A0F]/50">
        {active ? (
          <button
            onClick={clearImpact}
            className="h-9 flex-1 rounded-lg border border-white/10 text-xs font-semibold text-white/70 hover:bg-white/10 hover:text-white/95 border-0 bg-transparent cursor-pointer transition-colors"
          >
            Clear Simulation
          </button>
        ) : (
          <button
            onClick={() => simulateImpact(path)}
            className="flex h-9 flex-1 items-center justify-center gap-1.5 rounded-lg bg-rose-600 hover:bg-rose-500 text-xs font-semibold text-white shadow-lg shadow-rose-950/40 cursor-pointer border-0 transition-all"
          >
            <Zap size={13} /> Simulate Impact
          </button>
        )}
      </div>
    </>
  )
}

function ActionRow({ path }: { path: string }) {
  const simulateImpact = useGraphStore((s) => s.simulateImpact)
  const activeProjectRoot = useGraphStore((s) => s.activeProjectRoot)

  const handleOpenInEditor = useCallback(async () => {
    const invoke = tauriInvoke()
    if (invoke) {
      try {
        await invoke('open_in_editor', { path, projectRoot: activeProjectRoot })
      } catch (err) {
        console.error('Failed to open in editor:', err)
      }
    } else {
      window.open(`vscode://file/${encodeURI(path)}`, '_blank')
    }
  }, [path, activeProjectRoot])

  return (
    <div className="mt-3 flex gap-2 rounded-xl border border-white/10 p-2 bg-[#090A0F]/50">
      <button
        onClick={() => simulateImpact(path)}
        className="flex h-9 flex-1 items-center justify-center gap-1.5 rounded-lg bg-rose-600 hover:bg-rose-500 text-xs font-semibold text-white shadow-lg shadow-rose-950/40 cursor-pointer border-0 transition-all"
      >
        <Zap size={13} /> Simulate Impact
      </button>
      <button
        onClick={handleOpenInEditor}
        className="flex h-9 flex-1 items-center justify-center gap-1.5 rounded-lg border border-white/10 bg-white/[0.03] hover:bg-white/[0.08] text-xs font-semibold text-white/80 hover:text-white cursor-pointer border-0 transition-all"
      >
        <ExternalLink size={13} /> Open in Editor
      </button>
    </div>
  )
}

function Empty({ label }: { label: string }) {
  return <div className="px-2 py-1 text-xs text-white/30 italic">{label}</div>
}

function CallGraphTab() {
  const selectedSymbol = useGraphStore((s) => s.selectedSymbol)
  const setSelectedSymbol = useGraphStore((s) => s.setSelectedSymbol)
  const focusNode = useGraphStore((s) => s.focusNode)
  const activeProjectRoot = useGraphStore((s) => s.activeProjectRoot)
  
  const contextSymbols = useGraphStore((s) => s.contextSymbols)
  const addSymbolToContext = useGraphStore((s) => s.addSymbolToContext)
  const removeSymbolFromContext = useGraphStore((s) => s.removeSymbolFromContext)

  const [data, setData] = useState<CallGraph | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!selectedSymbol) {
      // Clearing a fetch result when its input goes away.
      // eslint-disable-next-line react-hooks/set-state-in-effect
      setData(null)
      return
    }
    let active = true
    const fetchCallGraph = async () => {
      setLoading(true)
      setError(null)
      try {
        const invoke = tauriInvoke()
        if (invoke) {
          const res = await invoke('get_symbol_call_graph', {
            path: selectedSymbol.path,
            symbol: selectedSymbol.name,
            projectRoot: activeProjectRoot,
          })
          if (active) {
            setData(res as CallGraph)
          }
        } else {
          setData({ callers: [], callees: [] })
        }
      } catch (err) {
        if (active) {
          setError(String(err))
        }
      } finally {
        if (active) {
          setLoading(false)
        }
      }
    }
    fetchCallGraph()
    return () => {
      active = false
    }
  }, [selectedSymbol, activeProjectRoot])

  if (!selectedSymbol) {
    return (
      <div className="text-center text-xs text-white/40 py-8 leading-relaxed">
        Select a symbol from the Overview tab exports/symbols and click the Bolt/Zap button to trace its calls.
      </div>
    )
  }

  const handleNodeClick = (file: string, sym: string) => {
    focusNode(file)
    setSelectedSymbol({ path: file, name: sym })
  }

  return (
    <div className="space-y-3">
      <div className="rounded-xl bg-violet-950/20 border border-violet-500/30 p-3 shadow-inner">
        <div className="text-[10px] uppercase font-bold tracking-wider text-violet-400">Traced Symbol</div>
        <div className="font-mono text-sm text-white/90 font-semibold mt-0.5 truncate" title={selectedSymbol.name}>{selectedSymbol.name}</div>
        <div className="font-mono text-[10px] text-white/40 truncate" title={selectedSymbol.path}>/{selectedSymbol.path}</div>
      </div>

      {loading && <div className="text-center py-4 text-xs text-white/40">Loading call tree…</div>}
      {error && <div className="text-center py-4 text-xs text-rose-400">Error: {error}</div>}

      {!loading && !error && data && (
        <>
          <Accordion title="Callers (invoked by)" count={data.callers.length} defaultOpen>
            {data.callers.length === 0 ? (
              <Empty label="No callers found" />
            ) : (
              <div className="space-y-1">
                {data.callers.map((c, i) => {
                  const refId = `${c.file_path}#${c.symbol_name}`
                  const isCurated = contextSymbols.has(refId)
                  return (
                    <div key={i} className="flex items-center justify-between hover:bg-white/[0.04] rounded-lg px-2 py-1 group transition-colors">
                      <button
                        onClick={() => handleNodeClick(c.file_path, c.symbol_name)}
                        className="truncate flex-1 min-w-0 mr-2 text-left bg-transparent border-0 p-0 font-mono text-xs text-white/70 hover:text-white cursor-pointer"
                      >
                        <ArrowUpRight size={10} className="inline mr-1 text-violet-400 shrink-0" />
                        <KindBadge kind={c.kind} muted />
                        <span>{c.symbol_name}</span>
                        <span className="text-[9px] text-white/30 block ml-3 truncate">/{c.file_path.split('/').pop()}</span>
                      </button>
                      <button
                        onClick={() => {
                          if (isCurated) {
                            removeSymbolFromContext(c.file_path, c.symbol_name)
                          } else {
                            addSymbolToContext(c.file_path, c.symbol_name)
                          }
                        }}
                        className={[
                          'rounded p-1 transition-colors shrink-0 opacity-0 group-hover:opacity-100 border-0 cursor-pointer',
                          isCurated ? 'opacity-100 bg-emerald-500/20 text-emerald-300' : 'text-white/40 hover:bg-white/10 hover:text-white/80 bg-transparent'
                        ].join(' ')}
                        title={isCurated ? 'Remove from prompt context' : 'Add to prompt context'}
                      >
                        {isCurated ? <Check size={10} /> : <Plus size={10} />}
                      </button>
                    </div>
                  )
                })}
              </div>
            )}
          </Accordion>

          <Accordion title="Callees (invokes)" count={data.callees.length} defaultOpen>
            {data.callees.length === 0 ? (
              <Empty label="No callees found" />
            ) : (
              <div className="space-y-1">
                {data.callees.map((c, i) => {
                  const refId = `${c.file_path}#${c.symbol_name}`
                  const isCurated = contextSymbols.has(refId)
                  return (
                    <div key={i} className="flex items-center justify-between hover:bg-white/[0.04] rounded-lg px-2 py-1 group transition-colors">
                      <button
                        onClick={() => handleNodeClick(c.file_path, c.symbol_name)}
                        className="truncate flex-1 min-w-0 mr-2 text-left bg-transparent border-0 p-0 font-mono text-xs text-white/70 hover:text-white cursor-pointer"
                      >
                        <ArrowDownRight size={10} className="inline mr-1 text-violet-400 shrink-0" />
                        <KindBadge kind={c.kind} muted />
                        <span>{c.symbol_name}</span>
                        <span className="text-[9px] text-white/30 block ml-3 truncate">/{c.file_path.split('/').pop()}</span>
                      </button>
                      <button
                        onClick={() => {
                          if (isCurated) {
                            removeSymbolFromContext(c.file_path, c.symbol_name)
                          } else {
                            addSymbolToContext(c.file_path, c.symbol_name)
                          }
                        }}
                        className={[
                          'rounded p-1 transition-colors shrink-0 opacity-0 group-hover:opacity-100 border-0 cursor-pointer',
                          isCurated ? 'opacity-100 bg-emerald-500/20 text-emerald-300' : 'text-white/40 hover:bg-white/10 hover:text-white/80 bg-transparent'
                        ].join(' ')}
                        title={isCurated ? 'Remove from prompt context' : 'Add to prompt context'}
                      >
                        {isCurated ? <Check size={10} /> : <Plus size={10} />}
                      </button>
                    </div>
                  )
                })}
              </div>
            )}
          </Accordion>
        </>
      )}
    </div>
  )
}

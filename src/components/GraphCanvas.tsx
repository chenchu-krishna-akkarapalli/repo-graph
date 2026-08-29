import { useCallback, useEffect, useRef, useState, Component, type ReactNode, type ErrorInfo } from 'react'
import ReactFlow, {
  Background,
  BackgroundVariant,
  Controls,
  MiniMap,
  ReactFlowProvider,
  useReactFlow,
  type Edge,
  type Node,
  type NodeMouseHandler,
} from 'reactflow'
import 'reactflow/dist/style.css'
import { useGraphStore } from '../store'
import { buildFlow, NODE_HEIGHT, NODE_WIDTH } from '../lib/layout'
import CustomFileNode from './nodes/CustomFileNode'
import CustomFolderNode from './nodes/CustomFolderNode'
import CustomSymbolNode from './nodes/CustomSymbolNode'
import EdgeHighlight from './nodes/EdgeHighlight'

interface ErrorBoundaryProps {
  children: ReactNode
}

interface ErrorBoundaryState {
  hasError: boolean
  error: Error | null
}

class GraphErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  constructor(props: ErrorBoundaryProps) {
    super(props)
    this.state = { hasError: false, error: null }
  }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { hasError: true, error }
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error('[GraphErrorBoundary] Caught canvas render exception:', error, errorInfo)
  }

  handleRehydrate = () => {
    this.setState({ hasError: false, error: null })
    void useGraphStore.getState().load()
  }

  render() {
    if (this.state.hasError) {
      return (
        <div className="flex h-full w-full items-center justify-center bg-[#07080B] p-6">
          <div className="max-w-md rounded-2xl border border-amber-500/40 bg-[#0F1218] p-6 text-center shadow-2xl backdrop-blur-md">
            <div className="mb-2 text-sm font-semibold text-amber-400">Graph Render Interrupted</div>
            <p className="text-xs text-white/60 leading-relaxed mb-4">
              {this.state.error?.message || 'A transient error occurred during graph canvas rendering.'}
            </p>
            <button
              onClick={this.handleRehydrate}
              className="inline-flex items-center gap-2 rounded-xl bg-violet-600 px-4 py-2 text-xs font-semibold text-white hover:bg-violet-500 transition-all cursor-pointer shadow-lg"
            >
              <span>Auto-rehydrate Graph</span>
            </button>
          </div>
        </div>
      )
    }

    return this.props.children
  }
}

const nodeTypes = { file: CustomFileNode, folder: CustomFolderNode, symbol: CustomSymbolNode }
const edgeTypes = { highlight: EdgeHighlight }

/** Node widths vary with rank, so centering has to read the laid-out width
 *  rather than assume the base one. */
function centerOf(node: Node): { x: number; y: number } {
  const width = typeof node.style?.width === 'number' ? node.style.width : NODE_WIDTH
  const height = typeof node.style?.height === 'number' ? node.style.height : NODE_HEIGHT
  return { x: node.position.x + width / 2, y: node.position.y + height / 2 }
}

/** Above this size, layout runs off the paint frame behind a progress overlay. */
const PROGRESSIVE_LOAD_THRESHOLD = 500
const EMPTY_FLOW: { nodes: Node[]; edges: Edge[] } = { nodes: [], edges: [] }

function Canvas() {
  const graph = useGraphStore((s) => s.graph)
  const languageFilters = useGraphStore((s) => s.languageFilters)
  const collapsedDirs = useGraphStore((s) => s.collapsedDirs)
  const expandedFiles = useGraphStore((s) => s.expandedFiles)
  const minRank = useGraphStore((s) => s.minRank)
  const densityMode = useGraphStore((s) => s.densityMode)
  const toggleSpotlightMode = useGraphStore((s) => s.toggleSpotlightMode)
  const select = useGraphStore((s) => s.select)
  const clearSelection = useGraphStore((s) => s.clearSelection)
  const setHovered = useGraphStore((s) => s.setHovered)
  const loadError = useGraphStore((s) => s.loadError)
  const agentActivity = useGraphStore((s) => s.agentActivity)
  const [isTelemetryExpanded, setIsTelemetryExpanded] = useState(true)
  const { setCenter, getZoom, fitView } = useReactFlow()

  // Structure-only state: hover/selection/impact never touch these arrays,
  // so React Flow's internal (zustand) store handles pan/drag/zoom without
  // re-entering React's render cycle for the whole graph.
  //
  // Past PROGRESSIVE_LOAD_THRESHOLD nodes, `buildFlow` is deferred one frame
  // so the browser can paint the ingest overlay instead of janking on layout.
  const [{ nodes, edges }, setFlow] = useState(EMPTY_FLOW)
  const [ingestingCount, setIngestingCount] = useState(0)
  // Nodes land a frame after ReactFlow mounts, so the `fitView` prop would
  // fit an empty graph. Refit imperatively when a *new graph* is ingested —
  // filter/collapse toggles keep the user's camera where it was.
  const fittedGraphRef = useRef<unknown>(null)

  useEffect(() => {
    if (!graph) {
      // eslint-disable-next-line react-hooks/set-state-in-effect
      setFlow(EMPTY_FLOW)
      setIngestingCount(0)
      fittedGraphRef.current = null
      return
    }
    const build = () =>
      buildFlow(graph, languageFilters, collapsedDirs, expandedFiles, minRank, densityMode)
    if (graph.nodes.length <= PROGRESSIVE_LOAD_THRESHOLD) {
      // Layout is deliberately state, not a `useMemo`: past the threshold
      // it is deferred a macrotask so the overlay can paint first.
      setFlow(build())
      setIngestingCount(0)
      return
    }
    setIngestingCount(graph.nodes.length)
    // A macrotask, not rAF: rAF is throttled to zero in a hidden window, which
    // would leave the canvas stuck on the overlay forever.
    const timer = setTimeout(() => {
      setFlow(build())
      setIngestingCount(0)
    }, 32)
    return () => clearTimeout(timer)
  }, [graph, languageFilters, collapsedDirs, expandedFiles, minRank, densityMode])

  useEffect(() => {
    if (!graph || nodes.length === 0 || fittedGraphRef.current === graph) return
    fittedGraphRef.current = graph
    const timer = setTimeout(() => fitView({ padding: 0.2, duration: 0 }), 0)
    return () => clearTimeout(timer)
  }, [graph, nodes, fitView])

  const onNodeClick = useCallback<NodeMouseHandler>(
    (event, node) => {
      if (node.type === 'file') select(node.id, event.shiftKey)
    },
    [select],
  )

  // Jakob's Law: double-click centers the node and opens its detail view
  // (200ms camera move keeps it inside the Doherty threshold).
  const onNodeDoubleClick = useCallback<NodeMouseHandler>(
    (_event, node: Node) => {
      if (node.type !== 'file') return
      select(node.id, false)
      const { x, y } = centerOf(node)
      setCenter(x, y, { zoom: Math.max(getZoom(), 1.1), duration: 200 })
    },
    [select, setCenter, getZoom],
  )

  // `setHovered` swaps the generated CSS rules synchronously and mirrors the
  // path into the store on the next frame — no component re-renders here.
  const onNodeMouseEnter = useCallback<NodeMouseHandler>(
    (_e, node) => {
      const path = node.type === 'file' ? node.id : (node.parentNode ?? null)
      setHovered(path)
    },
    [setHovered],
  )
  const onNodeMouseLeave = useCallback(() => setHovered(null), [setHovered])

  // Never leave the canvas stuck in a dimmed state if a node unmounts under
  // the cursor (virtualization can do this during a fast pan).
  useEffect(() => () => setHovered(null), [setHovered])

  // Explorer double-click → smooth 200ms center on the node (one-shot
  // request; the nonce distinguishes repeat focuses on the same path).
  const focusRequest = useGraphStore((s) => s.focusRequest)
  useEffect(() => {
    if (!focusRequest) return
    const node = nodes.find((n) => n.id === focusRequest.path)
    if (!node) return
    const { x, y } = centerOf(node)
    setCenter(x, y, { zoom: Math.max(getZoom(), 1.1), duration: 200 })
  }, [focusRequest, nodes, setCenter, getZoom])

  // Esc deselects, Alt+S toggles spotlight mode
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') clearSelection()
      if (e.altKey && (e.key.toLowerCase() === 's' || e.code === 'KeyS')) {
        e.preventDefault()
        toggleSpotlightMode()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [clearSelection, toggleSpotlightMode])

  if (loadError) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="max-w-md rounded-2xl border border-rose-500/40 bg-[#0F1218] p-6 text-center shadow-2xl backdrop-blur-md">
          <div className="mb-2 font-semibold text-rose-400">Graph cache unavailable</div>
          <p className="text-sm text-white/50 leading-relaxed">
            {loadError} — run{' '}
            <code className="font-mono text-xs text-white/80 bg-white/10 px-1.5 py-0.5 rounded">mcp_server index &lt;repo&gt;</code>{' '}
            and reload.
          </p>
        </div>
      </div>
    )
  }

  if (!graph) {
    return null
  }

  return (
    <div className="relative h-full w-full overflow-hidden bg-[#07080B]">
      <ReactFlow
        nodes={nodes}
        edges={edges}
        nodeTypes={nodeTypes}
        edgeTypes={edgeTypes}
        onNodeClick={onNodeClick}
        onNodeDoubleClick={onNodeDoubleClick}
        onNodeMouseEnter={onNodeMouseEnter}
        onNodeMouseLeave={onNodeMouseLeave}
        onPaneClick={clearSelection}
        // Jakob's Law canvas navigation: drag background to pan, wheel to
        // zoom, Shift+click (or Shift+drag box) to multi-select.
        panOnDrag
        zoomOnScroll
        multiSelectionKeyCode="Shift"
        selectionKeyCode="Shift"
        deleteKeyCode={null}
        nodesDraggable
        nodesConnectable={false}
        fitView
        minZoom={0.15}
        onlyRenderVisibleElements={true}
        proOptions={{ hideAttribution: false }}
        className="bg-[#07080B]"
      >
        <Background variant={BackgroundVariant.Dots} gap={28} size={1.5} color="#161B26" />
        <Controls showInteractive={false} position="bottom-left" />
        <MiniMap
          nodeColor={(n) => {
            if (n.data?.domainColorHex) return n.data.domainColorHex
            if (n.data?.language === 'javascript') return '#3B82F6'
            if (n.data?.language === 'python') return '#10B981'
            if (n.data?.language === 'rust') return '#F59E0B'
            return '#64748B'
          }}
          nodeStrokeWidth={2}
          maskColor="rgba(7, 8, 11, 0.75)"
          position="bottom-right"
          className="!bg-[#0F1218]/90 !border !border-white/10 !rounded-xl !shadow-2xl backdrop-blur-md mb-14 mr-4 !w-44 !h-28 overflow-hidden pointer-events-auto"
        />
      </ReactFlow>

      {/* Progressive ingest overlay — large graphs paint this before layout. */}
      {ingestingCount > 0 && (
        <div className="absolute inset-0 z-50 flex items-center justify-center bg-[#07080B]/60 backdrop-blur-md">
          <div className="w-80 rounded-2xl border border-white/10 bg-[#0F1218]/90 p-5 shadow-2xl backdrop-blur-xl">
            <div className="mb-3 text-xs font-semibold text-white/80">
              Ingesting {ingestingCount.toLocaleString()} nodes from SQLite WAL…
            </div>
            <div className="h-1.5 w-full overflow-hidden rounded-full bg-white/10">
              <div className="ingest-bar h-full w-1/3 rounded-full bg-gradient-to-r from-violet-500 to-cyan-400" />
            </div>
            <div className="mt-2 font-mono text-[10px] text-white/40">
              Computing layered layout · virtualized render
            </div>
          </div>
        </div>
      )}

      {/* Agent Telemetry Radar Panel */}
      <div className="absolute bottom-4 right-4 z-40 flex flex-col items-end pointer-events-auto">
        <button
          onClick={() => setIsTelemetryExpanded(!isTelemetryExpanded)}
          className="flex items-center gap-2 rounded-xl bg-[#0F1218]/90 border border-white/10 px-3.5 py-2 text-xs font-semibold text-white hover:bg-[#0F1218] hover:border-violet-500/50 transition-all shadow-xl backdrop-blur-md cursor-pointer"
        >
          <span className="flex h-2 w-2 rounded-full bg-violet-400 animate-ping" />
          <span>Agent Radar Telemetry {isTelemetryExpanded ? '▲' : '▼'}</span>
          {agentActivity.length > 0 && (
            <span className="rounded-full bg-violet-500/20 px-2 py-0.5 text-[10px] font-semibold text-violet-300">
              {agentActivity.length}
            </span>
          )}
        </button>

        {isTelemetryExpanded && (
          <div className="mt-2 w-80 max-h-72 overflow-y-auto rounded-2xl border border-white/10 bg-[#0F1218]/95 p-3.5 text-xs shadow-2xl backdrop-blur-md transition-all duration-300 flex flex-col gap-2">
            <div className="flex justify-between items-center border-b border-white/10 pb-2 text-[10px] font-bold uppercase tracking-wider text-white/40">
              <span>Live Agent Tool Actions</span>
              <span className="text-violet-400 font-mono">Telemetry active</span>
            </div>
            
            {agentActivity.length === 0 ? (
              <div className="py-8 text-center text-white/30 italic text-xs">
                Waiting for agent tool queries...
              </div>
            ) : (
              <div className="flex flex-col gap-2">
                {agentActivity.slice(0, 10).map((act) => {
                  const date = new Date(act.timestamp)
                  const timeStr = `${date.getHours().toString().padStart(2, '0')}:${date.getMinutes().toString().padStart(2, '0')}:${date.getSeconds().toString().padStart(2, '0')}`
                  
                  return (
                    <div key={act.id} className="flex flex-col gap-1 rounded-lg bg-[#07080B]/60 border border-white/5 p-2.5 transition-all hover:border-violet-500/40">
                      <div className="flex justify-between items-center text-[10px]">
                        <span className="font-semibold text-violet-300 uppercase tracking-tight">
                          {act.action}
                        </span>
                        <span className="text-white/30 font-mono">{timeStr}</span>
                      </div>
                      
                      {act.symbol && (
                        <div className="text-[11px] font-semibold text-white/90 font-mono truncate">
                          Symbol: <span className="text-cyan-300 bg-cyan-950/40 px-1.5 py-0.5 rounded-md border border-cyan-500/20">{act.symbol}</span>
                        </div>
                      )}
                      
                      {act.path && (
                        <div className="text-[10px] text-white/50 font-mono truncate" title={act.path}>
                          /{act.path}
                        </div>
                      )}
                    </div>
                  )
                })}
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  )
}

export default function GraphCanvas() {
  return (
    <GraphErrorBoundary>
      <ReactFlowProvider>
        <Canvas />
      </ReactFlowProvider>
    </GraphErrorBoundary>
  )
}

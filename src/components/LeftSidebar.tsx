import { memo, useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  ChevronDown,
  ChevronRight,
  FileCode2,
  FolderClosed,
  FolderOpen,
  FolderSearch,
  PanelLeftClose,
  PanelLeftOpen,
  Copy,
  Check,
  Share2,
} from 'lucide-react'
import { rankedFiles, useGraphStore } from '../store'
import { tauriInvoke } from '../lib/loadGraph'
import { httpMethodOf, METHOD_BADGE, METHOD_ORDER } from '../lib/httpMethod'
import { copyContextPrompt } from '../lib/promptExporter'
import { copyAsciiLayout } from '../lib/layoutExporter'
import type { FileTreeNode } from '../types'

type NavTab = 'explorer' | 'core' | 'routes' | 'context'

const NAV_TABS: { key: NavTab; label: string }[] = [
  { key: 'explorer', label: 'Explorer' },
  { key: 'core', label: 'Core' },
  { key: 'routes', label: 'Routes' },
  { key: 'context', label: 'Context' },
]

/** How many ranked files the Core tab lists — the UI's `top_k`. */
const CORE_LIST_LIMIT = 30

/** Shown when no workspace is open — never a machine-specific path. */
const PLACEHOLDER_ROOT = '/absolute/path/to/your/project'

/**
 * Mirrors `McpConfigSnippet` in `src-tauri/src/main.rs`. The binary path and
 * host-specific snippets are generated server-side — the backend is the only
 * place that can actually check the filesystem for where `mcp_server`
 * lives, which has nothing to do with which project the frontend has open.
 */
interface McpConfigSnippet {
  binary_path: string
  binary_exists: boolean
  claude_desktop_json: string
  codex_toml: string
  vscode_json: string
}

/** Reads `REPOGRAPH_MCP_TOOLS` back out of the already-generated Claude
 *  Desktop JSON so the CLI command line has one source of truth instead of
 *  a second hardcoded copy of the tool list. */
function mcpToolsEnv(snippet: McpConfigSnippet): string {
  try {
    const parsed = JSON.parse(snippet.claude_desktop_json)
    return parsed?.mcpServers?.['repo-graph']?.env?.REPOGRAPH_MCP_TOOLS ?? ''
  } catch {
    return ''
  }
}

/**
 * Workspace navigation drawer (left pane, 240–320px, default 288px).
 * Segmented tab switcher (Explorer / Routes / Context) with the Token
 * Burner Meter and MCP Connection Helper pinned to the bottom.
 */
export default function LeftSidebar() {
  const [open, setOpen] = useState(true)
  const [width, setWidth] = useState(288)
  const [tab, setTab] = useState<NavTab>('explorer')
  const [showIntegrationModal, setShowIntegrationModal] = useState(false)
  const [mcpSnippet, setMcpSnippet] = useState<McpConfigSnippet | null>(null)
  const [mcpSnippetError, setMcpSnippetError] = useState<string | null>(null)
  const dragging = useRef(false)
  const [recentProjects, setRecentProjects] = useState<{ path: string }[]>([])
  const [dropdownOpen, setDropdownOpen] = useState(false)

  const activeProjectRoot = useGraphStore((s) => s.activeProjectRoot)
  const fileTree = useGraphStore((s) => s.fileTree)
  const isIndexing = useGraphStore((s) => s.isIndexing)
  const openProject = useGraphStore((s) => s.openProject)
  const selectProject = useGraphStore((s) => s.selectProject)
  const graph = useGraphStore((s) => s.graph)
  const contextCount = useGraphStore((s) => s.contextFiles.size + s.contextSymbols.size)
  const nativeAvailable = tauriInvoke() !== null

  // Fetched fresh each time the modal opens (rather than on mount) so it
  // always reflects the currently open project's `args`, and re-detects the
  // binary in case the user just built it.
  useEffect(() => {
    if (!showIntegrationModal) return
    const invoke = tauriInvoke()
    if (!invoke) {
      // Browser dev build: report the unavailable-IPC state once.
      // eslint-disable-next-line react-hooks/set-state-in-effect
      setMcpSnippet(null)
      setMcpSnippetError(
        'Native path resolution is unavailable in the browser dev build — run the desktop app to see the real binary path.',
      )
      return
    }
    setMcpSnippet(null)
    setMcpSnippetError(null)
    invoke('get_mcp_config_snippet', { projectRoot: activeProjectRoot ?? PLACEHOLDER_ROOT })
      .then((res) => setMcpSnippet(res as McpConfigSnippet))
      .catch((e) => setMcpSnippetError(e instanceof Error ? e.message : String(e)))
  }, [showIntegrationModal, activeProjectRoot])

  const routesCount = useMemo(() => {
    if (!graph) return 0
    let n = 0
    for (const node of graph.nodes) {
      for (const sym of node.symbols ?? []) {
        if (sym.kind === 'route') n++
      }
    }
    return n
  }, [graph])

  const [layoutCopied, setLayoutCopied] = useState(false)
  const handleCopyLayout = async () => {
    try {
      await copyAsciiLayout(activeProjectRoot, fileTree)
      setLayoutCopied(true)
      setTimeout(() => setLayoutCopied(false), 1000)
    } catch (err) {
      console.error(err)
    }
  }

  const onPointerDown = useCallback((e: React.PointerEvent) => {
    dragging.current = true
    e.currentTarget.setPointerCapture(e.pointerId)
  }, [])
  const onPointerMove = useCallback((e: React.PointerEvent) => {
    if (dragging.current) setWidth(Math.min(320, Math.max(240, e.clientX)))
  }, [])
  const onPointerUp = useCallback(() => {
    dragging.current = false
  }, [])

  const loadRecents = async () => {
    const invoke = tauriInvoke()
    if (!invoke) return
    try {
      const resStr = (await invoke('get_recent_projects')) as string
      const config = JSON.parse(resStr)
      if (config && Array.isArray(config.projects)) {
        setRecentProjects(config.projects.slice(0, 5))
      }
    } catch (err) {
      console.error(err)
    }
  }

  const getFolderName = (fullPath: string) => {
    const clean = fullPath.replace(/\\/g, '/')
    return clean.split('/').pop() || fullPath
  }

  if (!open) {
    return (
      <button
        onClick={() => setOpen(true)}
        className="flex w-9 shrink-0 items-start justify-center border-r border-white/10 bg-[#0D1016] pt-3.5 text-white/40 hover:text-white/90 transition-colors border-0 cursor-pointer"
        title="Open file explorer"
      >
        <PanelLeftOpen size={16} />
      </button>
    )
  }

  return (
    <>
      <aside
        style={{ width }}
        className="relative flex shrink-0 flex-col border-r border-white/10 bg-[#0D1016]/95 backdrop-blur-md z-30"
      >
        <div className="flex items-center gap-2 border-b border-white/10 p-2.5">
          <button
            onClick={() => void openProject()}
            disabled={!nativeAvailable || isIndexing}
            title={
              nativeAvailable
                ? 'Select a project folder to index'
                : 'Available in the desktop app (npm run tauri dev)'
            }
            className="flex h-8 flex-1 items-center justify-center gap-2 rounded-md bg-violet-600 hover:bg-violet-500 text-xs font-medium text-white shadow-md shadow-violet-900/20 border-0 transition-all cursor-pointer disabled:cursor-not-allowed disabled:opacity-40"
          >
            <FolderSearch size={14} />
            {isIndexing ? 'Indexing…' : 'Open Folder'}
          </button>
          <button
            onClick={() => setOpen(false)}
            className="rounded-md p-2 text-white/40 hover:bg-white/[0.06] hover:text-white/90 transition-colors border-0 bg-transparent cursor-pointer"
            title="Collapse explorer"
          >
            <PanelLeftClose size={15} />
          </button>
        </div>

        {activeProjectRoot && (
          <div className="relative flex flex-col border-b border-white/10 bg-black/20">
            <div className="flex items-center justify-between px-3 py-1.5">
              <button
                onClick={() => {
                  setDropdownOpen(!dropdownOpen)
                  if (!dropdownOpen) void loadRecents()
                }}
                className="flex items-center gap-1 min-w-0 max-w-[80%] text-white/60 hover:text-white/95 cursor-pointer bg-transparent border-0 font-mono text-[10px] text-left transition-colors"
                title="Switch project workspace"
              >
                <span className="truncate">{getFolderName(activeProjectRoot)}</span>
                <ChevronDown size={10} className={`shrink-0 transition-transform duration-200 ${dropdownOpen ? 'rotate-180' : ''}`} />
              </button>

              <div className="flex items-center gap-1.5">
                <button
                  onClick={handleCopyLayout}
                  className={[
                    'p-1 rounded hover:bg-white/10 shrink-0 border-0 bg-transparent cursor-pointer transition-colors',
                    layoutCopied ? 'text-emerald-400 font-semibold' : 'text-white/40 hover:text-white/90'
                  ].join(' ')}
                  title="Copy project ASCII tree layout"
                >
                  {layoutCopied ? <Check size={11} /> : <Share2 size={11} />}
                </button>
              </div>
            </div>

            {/* Switcher Dropdown Overlay */}
            {dropdownOpen && (
              <div className="absolute top-full left-0 z-50 w-full border-b border-white/10 bg-[#0F1218] shadow-2xl flex flex-col max-h-48 overflow-y-auto backdrop-blur-md">
                <div className="px-3 py-1.5 text-[9px] font-semibold text-white/30 uppercase tracking-wider bg-black/40 border-b border-white/5 select-none">
                  Quick Switcher
                </div>
                {recentProjects.length === 0 ? (
                  <div className="px-3 py-2 text-[10px] text-white/30 italic">No recent workspaces</div>
                ) : (
                  recentProjects.map((p) => (
                    <button
                      key={p.path}
                      onClick={() => {
                        setDropdownOpen(false)
                        void selectProject(p.path)
                      }}
                      className={[
                        'px-3 py-1.5 text-left text-[10px] font-mono hover:bg-violet-500/15 truncate transition-colors w-full border-0 bg-transparent',
                        p.path.replace(/\\/g, '/') === activeProjectRoot.replace(/\\/g, '/')
                          ? 'text-violet-300 font-semibold bg-violet-500/10'
                          : 'text-white/60 hover:text-white/95 cursor-pointer'
                      ].join(' ')}
                      title={p.path}
                    >
                      {getFolderName(p.path)}
                      <span className="block text-[8px] text-white/30 truncate">{p.path}</span>
                    </button>
                  ))
                )}
              </div>
            )}
          </div>
        )}

        {/* Segmented tab switcher with active underline indicator */}
        <nav className="flex shrink-0 items-center border-b border-white/10 px-2" role="tablist">
          {NAV_TABS.map((t) => {
            const active = tab === t.key
            const count = t.key === 'routes' ? routesCount : t.key === 'context' ? contextCount : null
            return (
              <button
                key={t.key}
                role="tab"
                aria-selected={active}
                onClick={() => setTab(t.key)}
                className={[
                  'relative flex h-9 flex-1 items-center justify-center gap-1.5 border-0 bg-transparent text-xs font-medium transition-colors cursor-pointer',
                  active ? 'text-white' : 'text-white/40 hover:text-white/75',
                ].join(' ')}
              >
                {t.label}
                {count !== null && count > 0 && (
                  <span className="rounded-full bg-white/[0.06] px-1.5 py-0.5 font-mono text-[9px] text-white/50">
                    {count}
                  </span>
                )}
                <span
                  className={[
                    'absolute inset-x-2 bottom-0 h-0.5 rounded-full transition-all',
                    active ? 'bg-violet-500' : 'bg-transparent',
                  ].join(' ')}
                />
              </button>
            )
          })}
        </nav>

        <div className="min-h-0 flex-1 overflow-y-auto">
          {tab === 'explorer' && (
            <div className="p-1.5 space-y-0.5">
              {fileTree.length === 0 ? (
                <div className="px-3 py-4 text-xs text-white/35 leading-relaxed">
                  {nativeAvailable
                    ? 'No workspace open yet — pick a folder to explore its dependency graph.'
                    : 'Explorer tree requires the desktop app; the canvas below still shows the indexed graph.'}
                </div>
              ) : (
                fileTree.map((node) => <TreeRow key={node.path} node={node} depth={0} />)
              )}
            </div>
          )}
          {tab === 'core' && <CoreTab />}
          {tab === 'routes' && <RoutesTab />}
          {tab === 'context' && <ContextTab />}
        </div>

        {/* Token Burner Meter & Agent Integration — pinned bottom */}
        <div className="border-t border-white/10 bg-[#07080B]/50 p-3 space-y-3">
          <TokenBurnerMeter />
          <button
            onClick={() => setShowIntegrationModal(true)}
            className="flex h-8 w-full items-center justify-center gap-1.5 rounded-lg border border-white/10 bg-white/[0.03] text-xs font-medium text-white/70 hover:bg-white/[0.08] hover:text-white/90 transition-all cursor-pointer"
          >
            <svg className="h-3.5 w-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path strokeLinecap="round" strokeLinejoin="round" d="M9 3v2m6-2v2M9 19v2m6-2v2M5 9H3m2 6H3m18-6h-2m2 6h-2M7 19h10a2 2 0 002-2V7a2 2 0 00-2-2H7a2 2 0 00-2 2v10a2 2 0 002 2zM9 9h6v6H9V9z" />
            </svg>
            Integrate Agent
          </button>
          <AgentScaffoldStatus />
        </div>

        <div
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          className="absolute inset-y-0 -right-1 z-10 w-2 cursor-col-resize hover:bg-violet-500/30 transition-colors"
        />
      </aside>

      {showIntegrationModal && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-md p-4">
          <div className="max-h-[88vh] w-full max-w-xl overflow-y-auto rounded-2xl border border-white/10 bg-[#0F1218] p-5 shadow-2xl text-left text-white backdrop-blur-xl">
            <div className="flex items-center justify-between border-b border-white/10 pb-3">
              <h3 className="text-sm font-semibold text-white/90">Integrate Agent (MCP Config)</h3>
              <button
                onClick={() => setShowIntegrationModal(false)}
                className="rounded-lg p-1 text-white/40 hover:bg-white/10 hover:text-white/90 cursor-pointer border-0 bg-transparent transition-colors"
              >
                ✕
              </button>
            </div>

            <div className="mt-4 space-y-4 text-xs">
              <p className="text-white/60">
                Add Repo Graph to your AI coding agent using the configurations below — the binary
                path is resolved on this machine, not guessed from the open project.
              </p>

              {mcpSnippetError && (
                <div className="rounded-lg border border-amber-500/25 bg-amber-500/[0.06] px-3 py-2 text-[11px] text-amber-300">
                  {mcpSnippetError}
                </div>
              )}

              {!mcpSnippetError && !mcpSnippet && (
                <div className="flex items-center gap-2 rounded-lg border border-white/10 bg-white/[0.03] px-3 py-2 text-[11px] text-white/50">
                  <div className="h-3 w-3 animate-spin rounded-full border-2 border-violet-400 border-t-transparent" />
                  Resolving mcp_server on this machine…
                </div>
              )}

              {mcpSnippet && (
                <>
                  <div
                    className={[
                      'flex items-center justify-between gap-3 rounded-lg border px-3 py-2 text-[11px]',
                      mcpSnippet.binary_exists
                        ? 'border-emerald-500/25 bg-emerald-500/[0.06] text-emerald-300'
                        : 'border-rose-500/25 bg-rose-500/[0.06] text-rose-300',
                    ].join(' ')}
                  >
                    <span className="font-semibold">
                      {mcpSnippet.binary_exists ? '✓ Binary Detected' : '⚠️ Binary Missing'}
                    </span>
                    <span className="truncate font-mono text-white/50">{mcpSnippet.binary_path}</span>
                  </div>
                  {!mcpSnippet.binary_exists && (
                    <p className="text-white/45 leading-relaxed">
                      Build it with <code className="text-white/70">cargo build --release --bin mcp_server</code>{' '}
                      (from <code className="text-white/70">src-tauri/</code>), or install the Repo Graph desktop
                      bundle, which ships it alongside the app.
                    </p>
                  )}

                  <div>
                    <span className="block font-medium text-white/80 mb-1">
                      1. Claude Desktop Config (`claude_desktop_config.json`)
                    </span>
                    <pre className="overflow-x-auto rounded-lg bg-[#07080B] p-3 font-mono text-[10px] text-white/80 border border-white/10 select-all">
                      {mcpSnippet.claude_desktop_json}
                    </pre>
                  </div>

                  <div>
                    <span className="block font-medium text-white/80 mb-1">
                      2. Codex Config (`.codex/config.toml`)
                    </span>
                    <pre className="overflow-x-auto rounded-lg bg-[#07080B] p-3 font-mono text-[10px] text-white/80 border border-white/10 select-all">
                      {mcpSnippet.codex_toml}
                    </pre>
                  </div>

                  <div>
                    <span className="block font-medium text-white/80 mb-1">
                      3. VS Code Config (`.vscode/mcp.json`)
                    </span>
                    <pre className="overflow-x-auto rounded-lg bg-[#07080B] p-3 font-mono text-[10px] text-white/80 border border-white/10 select-all">
                      {mcpSnippet.vscode_json}
                    </pre>
                  </div>

                  <div>
                    <span className="block font-medium text-white/80 mb-1">4. CLI Integration Command</span>
                    <div className="relative rounded-lg bg-[#07080B] p-3 font-mono text-[10px] text-white/80 border border-white/10 break-all select-all">
                      {`claude mcp add repo-graph --env REPOGRAPH_MCP_TOOLS="${mcpToolsEnv(mcpSnippet)}" -- "${mcpSnippet.binary_path}" "${activeProjectRoot ? activeProjectRoot.replace(/\\/g, '/') : PLACEHOLDER_ROOT}"`}
                    </div>
                  </div>

                  <p className="text-white/45 leading-relaxed">
                    Without the <code className="text-white/70">REPOGRAPH_MCP_TOOLS</code> variable the server
                    lists only <code className="text-white/70">repograph_explore</code>.
                  </p>
                </>
              )}
            </div>

            <div className="mt-5 flex justify-end border-t border-white/10 pt-3">
              <button
                onClick={() => setShowIntegrationModal(false)}
                className="rounded-lg bg-violet-600 px-4 py-1.5 text-xs font-semibold text-white hover:bg-violet-500 cursor-pointer border-0 shadow-lg shadow-violet-950/40 transition-colors"
              >
                Done
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  )
}

/**
 * The repo's architectural core, highest dependency-graph centrality first —
 * the same ordering `repograph_files` serves an agent, for a human.
 *
 * Also owns the canvas rank filter. The two belong together: the list answers
 * "what matters here", the slider applies that answer to the canvas, and the
 * node count under it is what stops the filter from quietly hiding the repo.
 */
function CoreTab() {
  const graph = useGraphStore((s) => s.graph)
  const focusNode = useGraphStore((s) => s.focusNode)
  const selected = useGraphStore((s) => s.selected)
  const minRank = useGraphStore((s) => s.minRank)
  const setMinRank = useGraphStore((s) => s.setMinRank)

  const ranked = useMemo(() => rankedFiles(graph, CORE_LIST_LIMIT), [graph])
  const totalRanked = useMemo(() => rankedFiles(graph).length, [graph])
  const visibleCount = useMemo(
    () => (graph?.nodes ?? []).filter((n) => (n.rank_score ?? 0) >= minRank).length,
    [graph, minRank],
  )
  const totalCount = graph?.nodes.length ?? 0

  if (totalRanked === 0) {
    return (
      <div className="px-3 py-4 text-xs leading-relaxed text-white/35">
        No ranking available. Re-index the workspace — ranks are computed on
        load, so an older cache picks them up on the next open.
      </div>
    )
  }

  return (
    <div className="space-y-3 p-2.5">
      <div className="space-y-1.5 rounded-lg border border-white/10 bg-[#121620]/70 p-2.5">
        <div className="flex items-center justify-between">
          <span className="text-[10px] font-bold uppercase tracking-wider text-white/50">
            Canvas Rank Filter
          </span>
          <button
            onClick={() => setMinRank(0)}
            disabled={minRank === 0}
            className="border-0 bg-transparent p-0 text-[9px] text-violet-400 transition-opacity hover:underline disabled:opacity-30 cursor-pointer"
          >
            reset
          </button>
        </div>
        <input
          type="range"
          min={0}
          max={0.9}
          step={0.01}
          value={minRank}
          onChange={(e) => setMinRank(Number(e.target.value))}
          aria-label="Minimum node rank shown on the canvas"
          className="h-1.5 w-full cursor-pointer appearance-none rounded-full bg-white/10 accent-violet-500"
        />
        <div className="flex justify-between font-mono text-[9px] text-white/40">
          <span>Rank ≥ {minRank.toFixed(2)}</span>
          <span className={visibleCount < totalCount ? 'text-amber-300' : ''}>
            {visibleCount.toLocaleString()} / {totalCount.toLocaleString()} nodes
          </span>
        </div>
      </div>

      <div>
        <div className="px-1 pb-1 font-mono text-[9px] font-semibold text-white/35">
          TOP {Math.min(CORE_LIST_LIMIT, totalRanked)} OF {totalRanked.toLocaleString()}
        </div>
        <div className="space-y-1">
          {ranked.map((n) => {
            const active = selected.has(n.path)
            const hidden = (n.rank_score ?? 0) < minRank
            return (
              <button
                key={n.path}
                onClick={() => focusNode(n.path)}
                title={`${n.path}\ncentrality ${(n.rank_score ?? 0).toFixed(3)} · ${n.in_degree} inbound edges`}
                className={[
                  'flex h-7 w-full cursor-pointer items-center gap-1.5 rounded-lg border px-2 text-left transition-all',
                  active
                    ? 'border-violet-500/40 bg-violet-500/15'
                    : 'border-white/[0.05] bg-white/[0.02] hover:border-white/10 hover:bg-white/[0.06]',
                  hidden ? 'opacity-35' : '',
                ].join(' ')}
              >
                <span className="w-7 shrink-0 rounded bg-white/[0.06] text-center font-mono text-[9px] tabular-nums text-white/45">
                  #{n.rank_order}
                </span>
                <span
                  className={`truncate font-mono text-[11px] ${active ? 'font-semibold text-violet-300' : 'text-white/80'}`}
                >
                  {n.path.split('/').pop()}
                </span>
                <span className="ml-auto shrink-0 font-mono text-[9px] text-white/30">
                  {n.in_degree > 0 ? `↓${n.in_degree}` : ''}
                </span>
              </button>
            )
          })}
        </div>
      </div>
    </div>
  )
}

/**
 * Every symbol of kind `route`, grouped by derived HTTP method.
 */
function RoutesTab() {
  const graph = useGraphStore((s) => s.graph)
  const focusNode = useGraphStore((s) => s.focusNode)
  const setSelectedSymbol = useGraphStore((s) => s.setSelectedSymbol)
  const setSidebarTab = useGraphStore((s) => s.setSidebarTab)
  const expandedFiles = useGraphStore((s) => s.expandedFiles)
  const toggleExpandFile = useGraphStore((s) => s.toggleExpandFile)
  const selectedSymbol = useGraphStore((s) => s.selectedSymbol)

  const grouped = useMemo(() => {
    const byMethod = new Map<string, { url: string; file: string }[]>()
    if (!graph) return byMethod
    for (const node of graph.nodes) {
      for (const sym of node.symbols ?? []) {
        if (sym.kind !== 'route') continue
        const method = httpMethodOf(sym.name)
        const list = byMethod.get(method) ?? []
        list.push({ url: sym.name, file: node.path })
        byMethod.set(method, list)
      }
    }
    for (const list of byMethod.values()) {
      list.sort((a, b) => a.url.localeCompare(b.url))
    }
    return byMethod
  }, [graph])

  const total = [...grouped.values()].reduce((n, list) => n + list.length, 0)

  const openRoute = (file: string, url: string) => {
    if (!expandedFiles.has(file)) toggleExpandFile(file)
    focusNode(file)
    setSelectedSymbol({ path: file, name: url })
    setSidebarTab('callgraph')
  }

  return (
    <div className="space-y-2.5 p-2.5">
      {total === 0 ? (
        <div className="py-4 text-center text-[10px] italic text-white/30">
          No API endpoints detected in this workspace.
        </div>
      ) : (
        METHOD_ORDER.filter((m) => grouped.has(m)).map((method) => (
          <div key={method}>
            <div className="px-1 pb-1 font-mono text-[9px] font-semibold text-white/35">
              {method} · {grouped.get(method)!.length}
            </div>
            <div className="space-y-1">
              {grouped.get(method)!.map(({ url, file }) => {
                const active = selectedSymbol?.path === file && selectedSymbol?.name === url
                return (
                  <button
                    key={`${file}#${url}`}
                    onClick={() => openRoute(file, url)}
                    title={`${file} — trace call graph`}
                    className={[
                      'flex h-7 w-full cursor-pointer items-center gap-1.5 rounded-lg border border-white/[0.05] px-2 text-left transition-all',
                      active ? 'bg-emerald-500/15 border-emerald-500/40' : 'bg-white/[0.02] hover:bg-white/[0.06] hover:border-white/10',
                    ].join(' ')}
                  >
                    <span
                      className={`shrink-0 rounded border px-1 font-mono text-[8px] font-bold leading-3 ${METHOD_BADGE[method] ?? METHOD_BADGE.GET}`}
                    >
                      {method}
                    </span>
                    <span
                      className={`truncate font-mono text-[11px] ${active ? 'text-emerald-300 font-semibold' : 'text-white/80'}`}
                    >
                      {url}
                    </span>
                    <span className="ml-auto shrink-0 truncate font-mono text-[9px] text-white/30">
                      {file.split('/').pop()}
                    </span>
                  </button>
                )
              })}
            </div>
          </div>
        ))
      )}
    </div>
  )
}

function ContextTab() {
  const graph = useGraphStore((s) => s.graph)
  const contextFiles = useGraphStore((s) => s.contextFiles)
  const removeFileFromContext = useGraphStore((s) => s.removeFileFromContext)
  const contextSymbols = useGraphStore((s) => s.contextSymbols)
  const removeSymbolFromContext = useGraphStore((s) => s.removeSymbolFromContext)
  const clearContextWorkspace = useGraphStore((s) => s.clearContextWorkspace)

  const [copied, setCopied] = useState(false)

  const handleCopyPrompt = async () => {
    try {
      await copyContextPrompt(contextFiles, contextSymbols)
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    } catch (err) {
      console.error(err)
    }
  }

  const { totalContextTokens, budgetPercent } = useMemo(() => {
    if (contextFiles.size === 0 && contextSymbols.size === 0) {
      return { totalContextTokens: 0, budgetPercent: 0 }
    }

    let bytes = 0
    if (graph) {
      for (const path of contextFiles) {
        const node = graph.nodes.find(n => n.path === path)
        if (node) {
          bytes += node.size_bytes
        }
      }
      for (const symRef of contextSymbols) {
        const idx = symRef.indexOf('#')
        if (idx !== -1) {
          const path = symRef.substring(0, idx)
          const name = symRef.substring(idx + 1)
          if (contextFiles.has(path)) continue // already counted full file
          const node = graph.nodes.find(n => n.path === path)
          if (node) {
            const sym = node.symbols?.find(s => s.name === name)
            if (sym) {
              const estBytes = (sym.end_line - sym.start_line + 1) * 40
              bytes += Math.min(node.size_bytes, estBytes)
            }
          }
        }
      }
    }
    const files = Math.round(bytes / 3.7)

    let manifestChars = 0
    if (graph) {
      const subgraphSet = new Set<string>()
      const curatedFiles = new Set<string>(contextFiles)
      for (const symRef of contextSymbols) {
        const idx = symRef.indexOf('#')
        if (idx !== -1) {
          curatedFiles.add(symRef.substring(0, idx))
        }
      }
      for (const path of curatedFiles) {
        subgraphSet.add(path)
        const deps = graph.edges
          .filter(e => e.from_path === path && e.kind !== 'route')
          .map(e => e.to_path)
        for (const d of deps) subgraphSet.add(d)
        const reqs = graph.edges
          .filter(e => e.to_path === path && e.kind !== 'route')
          .map(e => e.from_path)
        for (const r of reqs) subgraphSet.add(r)
      }

      for (const path of subgraphSet) {
        const node = graph.nodes.find(n => n.path === path)
        if (node) {
          manifestChars += node.path.length + 3
          if (node.exports && node.exports.length > 0) {
            manifestChars += 12 + node.exports.join(', ').length
          }
          if (node.routes && node.routes.length > 0) {
            manifestChars += 10 + node.routes.join(', ').length
          }
          const deps = graph.edges.filter(e => e.from_path === node.path && e.kind !== 'route')
          if (deps.length > 0) {
            manifestChars += 14 + deps.map(d => '/' + d.to_path).join(', ').length
          }
          manifestChars += 1
        }
      }
    }
    const manifest = Math.round(manifestChars / 3.7)
    const total = files + manifest
    const budget = Math.max(0, Math.min(100, (total / 16000) * 100))

    return {
      totalContextTokens: total,
      budgetPercent: Math.round(budget * 10) / 10
    }
  }, [contextFiles, contextSymbols, graph])

  return (
    <div className="p-3 space-y-2.5">
      {contextFiles.size === 0 && contextSymbols.size === 0 ? (
        <div className="text-[10px] text-white/30 italic py-4 text-center leading-relaxed">
          Hover explorer rows or click symbol buttons to curate context.
        </div>
      ) : (
        <>
          <div className="flex items-center justify-between">
            <span className="text-[10px] font-bold tracking-wider text-white/50 uppercase">
              Curated ({contextFiles.size + contextSymbols.size})
            </span>
            <button
              onClick={clearContextWorkspace}
              className="text-[9px] text-violet-400 hover:underline lowercase font-normal cursor-pointer bg-transparent border-0 p-0"
            >
              clear
            </button>
          </div>

          {contextFiles.size > 0 && (
            <div className="max-h-40 overflow-y-auto space-y-1 pr-1">
              {[...contextFiles].map((path) => {
                const node = graph?.nodes.find((n) => n.path === path)
                const tokens = node ? Math.round(node.size_bytes / 3.7) : 0
                const filename = path.split('/').pop() || path
                return (
                  <div
                    key={path}
                    className="flex items-center justify-between rounded-lg bg-[#07080B]/60 px-2.5 py-1 text-xs border border-white/10 hover:border-white/20 transition-all group/row"
                  >
                    <div className="truncate flex-1 min-w-0 mr-2">
                      <span className="font-mono text-white/85 text-[11px] block truncate" title={filename}>{filename}</span>
                      <span className="text-[9px] text-white/35 font-mono truncate block" title={path}>/{path}</span>
                    </div>
                    <div className="flex items-center gap-1.5 shrink-0">
                      <span className="text-[9px] text-white/40 font-mono">
                        {tokens.toLocaleString()} t
                      </span>
                      <button
                        onClick={() => removeFileFromContext(path)}
                        className="text-white/40 hover:text-rose-400 cursor-pointer border-0 bg-transparent p-0 flex items-center text-[10px] font-bold transition-colors"
                        title="Remove file"
                      >
                        ✕
                      </button>
                    </div>
                  </div>
                )
              })}
            </div>
          )}

          {contextSymbols.size > 0 && (
            <div className="max-h-40 overflow-y-auto space-y-1 pr-1 border-t border-white/10 pt-1.5">
              {[...contextSymbols].map((symRef) => {
                const [path, name] = symRef.split('#')
                const filename = path.split('/').pop() || path
                return (
                  <div
                    key={symRef}
                    className="flex items-center justify-between rounded-lg bg-violet-950/20 px-2.5 py-0.5 text-xs border border-violet-500/30 hover:border-violet-500/50 transition-all group/row"
                  >
                    <div className="truncate flex-1 min-w-0 mr-2">
                      <span className="font-mono text-violet-300 text-[10px] block truncate" title={name}>{name}</span>
                      <span className="text-[9px] text-white/35 font-mono truncate block" title={path}>/{filename}</span>
                    </div>
                    <button
                      onClick={() => removeSymbolFromContext(path, name)}
                      className="text-white/40 hover:text-rose-400 cursor-pointer border-0 bg-transparent p-0 flex items-center text-[10px] font-bold transition-colors"
                      title="Remove symbol"
                    >
                      ✕
                    </button>
                  </div>
                )
              })}
            </div>
          )}

          {/* Progress Bar budget */}
          <div className="space-y-1">
            <div className="flex justify-between font-mono text-[9px] text-white/40">
              <span>Context: {totalContextTokens.toLocaleString()} / 16k t</span>
              <span className={budgetPercent > 90 ? 'text-rose-400 font-semibold' : 'text-white/40'}>
                {budgetPercent}%
              </span>
            </div>
            <div className="relative h-1.5 w-full overflow-hidden rounded-full bg-white/10">
              <div
                className={[
                  'h-full rounded-full transition-all duration-300',
                  budgetPercent > 90 ? 'bg-rose-500' : budgetPercent > 70 ? 'bg-amber-400' : 'bg-gradient-to-r from-emerald-500 to-violet-500'
                ].join(' ')}
                style={{ width: `${Math.min(100, budgetPercent)}%` }}
              />
            </div>
          </div>

          {/* Copy prompt button */}
          <button
            onClick={handleCopyPrompt}
            className={[
              'flex h-8 w-full items-center justify-center gap-1.5 rounded-lg text-xs font-semibold text-white transition-all cursor-pointer border-0 shadow-md',
              copied ? 'bg-emerald-600' : 'bg-violet-600 hover:bg-violet-500'
            ].join(' ')}
          >
            {copied ? <Check size={13} /> : <Copy size={13} />}
            {copied ? 'Copied Prompt!' : 'Copy Context Prompt'}
          </button>
        </>
      )}
    </div>
  )
}

/**
 * Playbook §28 status: is `.myrepograph-agent/` present in the open workspace?
 *
 * The scaffolding runs automatically on index, so the usual state is "active"
 * and this card is a quiet confirmation. It earns its space in the two cases
 * where the automatic run did not happen — a pre-existing project indexed
 * before §28 shipped, a read-only checkout, or `REPOGRAPH_NO_SCAFFOLD=1` —
 * where it becomes the one-click fix.
 */
function AgentScaffoldStatus() {
  const activeProjectRoot = useGraphStore((s) => s.activeProjectRoot)
  const [hasScaffold, setHasScaffold] = useState<boolean | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const refresh = useCallback(async (root: string | null) => {
    const invoke = tauriInvoke()
    if (!invoke || !root) {
      setHasScaffold(null) // browser dev build: nothing to report
      return
    }
    try {
      setHasScaffold((await invoke('check_agent_scaffold', { projectRoot: root })) as boolean)
    } catch {
      setHasScaffold(null)
    }
  }, [])

  // Re-check whenever the workspace changes — the status belongs to the
  // project, not to the session.
  useEffect(() => {
    // Async IPC probe of on-disk scaffold state; not derivable in render.
    // eslint-disable-next-line react-hooks/set-state-in-effect
    void refresh(activeProjectRoot)
  }, [activeProjectRoot, refresh])

  const handleSetup = async () => {
    const invoke = tauriInvoke()
    if (!invoke || !activeProjectRoot || busy) return
    setBusy(true)
    setError(null)
    try {
      await invoke('trigger_agent_scaffold', { projectRoot: activeProjectRoot })
      await refresh(activeProjectRoot)
    } catch (e) {
      // Surface the failure: the user clicked a button and is owed an answer.
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }

  if (hasScaffold === null) return null

  if (hasScaffold) {
    return (
      <div
        className="mt-3 flex items-center gap-2 rounded-lg border border-white/10 bg-[#121620]/60 p-2.5 text-xs backdrop-blur-sm"
        title=".myrepograph-agent/ configured for context engineering"
      >
        <span className="h-2 w-2 shrink-0 rounded-full bg-emerald-400 shadow-[0_0_8px_rgba(52,211,153,0.6)]" />
        <span className="truncate text-white/70">Agent Workspace Active</span>
      </div>
    )
  }

  return (
    <div className="mt-3 rounded-lg border border-white/10 bg-[#121620]/60 p-2.5 text-xs backdrop-blur-sm">
      <button
        onClick={handleSetup}
        disabled={busy}
        title="Create .myrepograph-agent/ with context-engineering defaults"
        className="flex w-full items-center gap-2 border-0 bg-transparent p-0 text-left text-xs text-amber-300/90 hover:text-amber-200 disabled:cursor-wait disabled:opacity-60 cursor-pointer transition-colors"
      >
        <span className="h-2 w-2 shrink-0 rounded-full bg-amber-400 shadow-[0_0_8px_rgba(245,158,11,0.6)]" />
        <span className="truncate">
          {busy ? 'Setting up…' : 'Agent Workspace Missing (Click to Setup)'}
        </span>
      </button>
      {error && (
        <div className="mt-1.5 border-t border-white/10 pt-1.5 text-[10px] leading-snug text-rose-300/90">
          {error}
        </div>
      )}
    </div>
  )
}

function TokenBurnerMeter() {
  const graph = useGraphStore((s) => s.graph)
  const readFiles = useGraphStore((s) => s.readFiles)
  const clearReadFiles = useGraphStore((s) => s.clearReadFiles)

  const { baselineTokens, activeTokens, savingsPercent } = useMemo(() => {
    if (!graph || graph.nodes.length === 0) {
      return { baselineTokens: 0, activeTokens: 0, savingsPercent: 0 }
    }

    let totalBytes = 0
    for (const n of graph.nodes) {
      totalBytes += n.size_bytes
    }
    const baseline = Math.round(totalBytes / 3.7)

    let manifestChars = 0
    for (const n of graph.nodes) {
      manifestChars += n.path.length + 3
      if (n.exports && n.exports.length > 0) {
        manifestChars += 12 + n.exports.join(', ').length
      }
      if (n.routes && n.routes.length > 0) {
        manifestChars += 10 + n.routes.join(', ').length
      }
      const deps = graph.edges?.filter(e => e.from_path === n.path && e.kind !== 'route') || []
      if (deps.length > 0) {
        manifestChars += 14 + deps.map(d => '/' + d.to_path).join(', ').length
      }
      manifestChars += 1
    }
    const manifestTokens = Math.round(manifestChars / 3.7)

    let readBytes = 0
    for (const path of readFiles) {
      const node = graph.nodes.find(n => n.path === path)
      if (node) {
        readBytes += node.size_bytes
      }
    }
    const readTokens = Math.round(readBytes / 3.7)

    const active = manifestTokens + readTokens
    const savings = baseline > 0 ? Math.max(0, Math.min(100, (1 - active / baseline) * 100)) : 0

    return {
      baselineTokens: baseline,
      activeTokens: active,
      savingsPercent: Math.round(savings * 10) / 10
    }
  }, [graph, readFiles])

  return (
    <div className="rounded-lg border border-white/10 bg-[#121620]/80 p-3 space-y-2 text-left text-white backdrop-blur-sm">
      <div className="flex items-center justify-between">
        <span className="text-[10px] font-bold tracking-wider text-white/50 uppercase">Token Meter</span>
        <button
          onClick={clearReadFiles}
          disabled={readFiles.size === 0}
          className="text-[9px] text-violet-400 hover:underline disabled:opacity-30 bg-transparent border-0 cursor-pointer p-0 transition-opacity"
        >
          Reset Simulation
        </button>
      </div>

      <div className="relative h-2 w-full overflow-hidden rounded-full bg-white/10">
        <div
          className="h-full rounded-full bg-gradient-to-r from-emerald-500 via-teal-500 to-violet-500 transition-all duration-300"
          style={{ width: `${savingsPercent}%` }}
        />
      </div>

      <div className="flex justify-between font-mono text-[9px] text-white/40">
        <div>
          <span className="text-white/60">Active:</span> {activeTokens.toLocaleString()} t
        </div>
        <div className="text-right">
          <span className="text-white/60">Baseline:</span> {baselineTokens.toLocaleString()} t
        </div>
      </div>
      <div className="text-center font-mono text-[10px] text-emerald-400 font-semibold">
        {savingsPercent}% Saved
      </div>
    </div>
  )
}

const TreeRow = memo(function TreeRow({ node, depth }: { node: FileTreeNode; depth: number }) {
  const [expanded, setExpanded] = useState(depth === 0)
  const select = useGraphStore((s) => s.select)
  const focusNode = useGraphStore((s) => s.focusNode)
  const isSelected = useGraphStore((s) => !node.is_dir && s.selected.has(node.path))
  const inGraph = useGraphStore(
    (s) => node.is_dir || (s.graph?.nodes.some((n) => n.path === node.path) ?? false),
  )
  const isRead = useGraphStore((s) => s.readFiles.has(node.path))
  const toggleReadFile = useGraphStore((s) => s.toggleReadFile)

  const isInContext = useGraphStore((s) => s.contextFiles.has(node.path))
  const addFileToContext = useGraphStore((s) => s.addFileToContext)

  const indent = { paddingLeft: 10 + depth * 14 }

  if (node.is_dir) {
    return (
      <div>
        <button
          onClick={() => setExpanded((e) => !e)}
          style={indent}
          className="flex w-full items-center gap-2 py-1.5 pr-2.5 text-left text-xs rounded-md text-white/65 hover:bg-white/[0.04] hover:text-white/95 border-0 cursor-pointer bg-transparent transition-colors"
          aria-expanded={expanded}
        >
          {expanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
          {expanded ? (
            <FolderOpen size={13} className="text-slate-400" />
          ) : (
            <FolderClosed size={13} className="text-slate-400" />
          )}
          <span className="truncate">{node.name}</span>
        </button>
        {expanded &&
          node.children.map((child) => (
            <TreeRow key={child.path} node={child} depth={depth + 1} />
          ))}
      </div>
    )
  }

  return (
    <button
      onClick={() => select(node.path, false)}
      onDoubleClick={() => focusNode(node.path)}
      style={indent}
      disabled={!inGraph}
      title={inGraph ? node.path : `${node.path} (not in graph index)`}
      className={[
        'group flex w-full items-center gap-2 py-1.5 pr-2.5 text-left text-xs border-0 bg-transparent relative rounded-md transition-colors',
        isSelected ? 'bg-violet-500/15 text-violet-300 font-medium' : 'text-white/80',
        inGraph ? 'hover:bg-white/[0.04] hover:text-white/95 cursor-pointer' : 'opacity-40',
      ].join(' ')}
    >
      <span className="w-1" />
      {inGraph && (
        <input
          type="checkbox"
          checked={isRead}
          onClick={(e) => e.stopPropagation()}
          onChange={() => toggleReadFile(node.path)}
          className="h-3.5 w-3.5 rounded border-white/20 bg-[#07080B] text-violet-500 focus:ring-0 cursor-pointer accent-violet-500"
          title="Toggle simulated ingestion (read status)"
        />
      )}
      <FileCode2 size={13} className="shrink-0 opacity-60" />
      <span className="truncate flex-1 min-w-0 mr-4">{node.name}</span>

      {inGraph && !isInContext && (
        <span
          onClick={(e) => {
            e.stopPropagation()
            addFileToContext(node.path)
          }}
          className="absolute right-2 opacity-0 group-hover:opacity-100 flex items-center justify-center h-4 w-4 rounded bg-violet-600 text-white hover:bg-violet-500 text-[10px] font-bold cursor-pointer transition-all"
          title="Add to Context Workspace"
        >
          +
        </span>
      )}
    </button>
  )
})

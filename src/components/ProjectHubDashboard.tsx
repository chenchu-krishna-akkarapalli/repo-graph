import { useEffect, useState } from 'react'
import { FolderOpen, History, Cpu, FileCode2, Clock } from 'lucide-react'
import { useGraphStore } from '../store'
import type { IndexPhase } from '../store'
import { tauriInvoke } from '../lib/loadGraph'

interface ProjectStats {
  files: number
  symbols: number
  language: string
}

interface RecentProject {
  path: string
  last_opened: string
  stats: ProjectStats
}

const PHASE_LABELS: Record<IndexPhase, string> = {
  idle: 'Preparing…',
  walking: 'Scanning directory tree…',
  parsing: 'Parsing AST symbols and imports…',
  db_write: 'Constructing SQLite dependency index…',
  complete: 'Index ready',
}

function formatBytes(n: number): string {
  if (n <= 0) return '0 B'
  if (n < 1024) return `${n} B`
  const units = ['KB', 'MB', 'GB', 'TB']
  let value = n / 1024
  let i = 0
  while (value >= 1024 && i < units.length - 1) {
    value /= 1024
    i++
  }
  return `${value.toFixed(1)} ${units[i]}`
}

function formatEta(seconds: number): string {
  if (seconds < 1) return '<1s'
  if (seconds < 60) return `~${Math.round(seconds)}s`
  const mins = Math.floor(seconds / 60)
  const secs = Math.round(seconds % 60)
  return `~${mins}m ${secs}s`
}

function formatRelativeTime(dateStr: string): string {
  try {
    const date = new Date(dateStr)
    const now = new Date()
    const diffMs = now.getTime() - date.getTime()
    if (isNaN(diffMs) || diffMs < 0) return 'recently'

    const diffSecs = Math.floor(diffMs / 1000)
    const diffMins = Math.floor(diffSecs / 60)
    const diffHours = Math.floor(diffMins / 60)
    const diffDays = Math.floor(diffHours / 24)

    if (diffSecs < 60) return 'just now'
    if (diffMins < 60) return `${diffMins}m ago`
    if (diffHours < 24) return `${diffHours}h ago`
    return `${diffDays}d ago`
  } catch {
    return 'recently'
  }
}

export default function ProjectHubDashboard() {
  const openProject = useGraphStore((s) => s.openProject)
  const selectProject = useGraphStore((s) => s.selectProject)
  const isIndexing = useGraphStore((s) => s.isIndexing)
  const status = useGraphStore((s) => s.status)
  const loadError = useGraphStore((s) => s.loadError)
  const indexProgress = useGraphStore((s) => s.indexProgress)

  const [recentProjects, setRecentProjects] = useState<RecentProject[]>([])
  const [debounceMs, setDebounceMs] = useState<number>(2000)

  useEffect(() => {
    const loadRecents = async () => {
      const invoke = tauriInvoke()
      if (!invoke) return
      try {
        const resStr = (await invoke('get_recent_projects')) as string
        const config = JSON.parse(resStr)
        if (config && Array.isArray(config.projects)) {
          setRecentProjects(config.projects)
        }
      } catch (err) {
        console.error('Failed to load recent projects:', err)
      }
    }
    void loadRecents()
  }, [isIndexing])

  const handleUpdateDebounce = async (val: number) => {
    setDebounceMs(val)
    const invoke = tauriInvoke()
    if (invoke) {
      try {
        await invoke('set_watch_debounce', { ms: val })
      } catch (err) {
        console.error('Failed to update watch debounce:', err)
      }
    }
  }

  const getLanguageBadge = (lang: string) => {
    switch (lang.toLowerCase()) {
      case 'rust':
        return 'text-orange-300 bg-orange-500/10 border-orange-500/20'
      case 'typescript':
      case 'javascript':
        return 'text-blue-300 bg-blue-500/10 border-blue-500/20'
      case 'python':
        return 'text-emerald-300 bg-emerald-500/10 border-emerald-500/20'
      case 'go':
        return 'text-cyan-300 bg-cyan-500/10 border-cyan-500/20'
      default:
        return 'text-white/60 bg-white/[0.04] border-white/10'
    }
  }

  const getFolderName = (fullPath: string) => {
    const clean = fullPath.replace(/\\/g, '/')
    return clean.split('/').pop() || fullPath
  }

  return (
    <div className="flex min-h-screen flex-col bg-[#07090E] text-white font-sans selection:bg-blue-500/30 relative">
      <main className="relative z-10 flex flex-1 flex-col items-center justify-center p-8 max-w-4xl mx-auto w-full">
        {/* Header */}
        <div className="text-center mb-10 select-none">
          <div className="inline-flex items-center gap-2 rounded-md border border-white/10 bg-white/[0.03] px-3 py-1 text-[11px] font-mono text-white/60 uppercase tracking-widest mb-4">
            <Cpu size={12} className="text-blue-400" />
            Static Dependency & MCP Engine
          </div>
          <h1 className="text-3xl font-bold tracking-tight text-white">
            Repo Graph Hub
          </h1>
          <p className="mt-2.5 text-xs text-white/50 max-w-md mx-auto leading-relaxed">
            Index software architecture, query symbol call graphs, and cut AI agent token ingestion by 99% with offline-first static analysis.
          </p>
        </div>

        {/* Real-time Indexing Progress HUD */}
        {isIndexing ? (
          status !== 'indexing' ? (
            <div className="flex flex-col items-center gap-4 py-12">
              <div className="h-7 w-7 animate-spin rounded-full border-2 border-blue-500 border-t-transparent" />
              <div className="text-xs text-white/40 font-mono tracking-wider">
                INITIALIZING GRAPH DATA...
              </div>
            </div>
          ) : (
            <div className="w-full max-w-md flex flex-col items-center gap-5 py-8 border border-white/10 bg-[#0F131D]/80 rounded-2xl px-8 backdrop-blur-xl shadow-2xl">
              <div className="h-8 w-8 animate-spin rounded-full border-2 border-blue-500 border-t-transparent" />

              <div className="w-full flex flex-col gap-2">
                <div className="h-2 w-full overflow-hidden rounded-full bg-white/[0.05] border border-white/10 shimmer-bar">
                  <div
                    className="h-full rounded-full bg-gradient-to-r from-blue-500 to-cyan-400 transition-[width] duration-200 ease-out"
                    style={{
                      width:
                        indexProgress.filesTotal > 0
                          ? `${Math.min(100, Math.round((indexProgress.filesProcessed / indexProgress.filesTotal) * 100))}%`
                          : '4%',
                    }}
                  />
                </div>
                <div className="flex justify-between text-[10px] font-mono text-white/40 uppercase tracking-wider">
                  <span>{PHASE_LABELS[indexProgress.phase]}</span>
                  <span>
                    {indexProgress.filesTotal > 0
                      ? `${Math.min(100, Math.round((indexProgress.filesProcessed / indexProgress.filesTotal) * 100))}%`
                      : '—'}
                  </span>
                </div>
              </div>

              <div className="text-xs text-white/70 font-mono text-center leading-relaxed">
                {indexProgress.filesTotal > 0
                  ? `${indexProgress.filesProcessed.toLocaleString()} / ${indexProgress.filesTotal.toLocaleString()} files — ${formatBytes(indexProgress.bytesProcessed)} / ${formatBytes(indexProgress.bytesTotal)}`
                  : 'Scanning workspace…'}
              </div>

              {indexProgress.etaSeconds !== null && indexProgress.phase === 'parsing' && (
                <div className="text-[11px] text-blue-300 font-mono tracking-wide">
                  Remaining: {formatEta(indexProgress.etaSeconds)}
                  {indexProgress.speedFilesPerSec > 0 && ` (~${Math.round(indexProgress.speedFilesPerSec)} files/sec)`}
                </div>
              )}
            </div>
          )
        ) : (
          <div className="w-full flex flex-col gap-6">
            {/* Primary Action Button */}
            <div className="flex justify-center">
              <button
                onClick={() => void openProject()}
                className="group relative flex items-center gap-3 rounded-xl border border-blue-500/30 bg-blue-600/15 hover:bg-blue-600/25 px-6 py-3.5 text-sm font-semibold text-white transition-all hover:scale-[1.01] active:scale-[0.99] cursor-pointer shadow-lg shadow-blue-950/20"
              >
                <FolderOpen size={18} className="text-blue-400 group-hover:text-blue-300 transition-colors" />
                <span>Open Codebase Workspace</span>
              </button>
            </div>

            {loadError && (
              <div className="rounded-xl border border-rose-500/30 bg-rose-950/20 px-4 py-3 text-xs text-rose-300 text-center max-w-lg mx-auto">
                {loadError}
              </div>
            )}

            {/* Recent Projects List */}
            {recentProjects.length > 0 && (
              <div className="w-full max-w-xl mx-auto space-y-3 pt-4">
                <div className="flex items-center justify-between text-xs font-semibold text-white/50 uppercase tracking-wider px-1">
                  <span className="flex items-center gap-1.5">
                    <History size={13} />
                    Recent Workspaces
                  </span>
                  <span className="font-mono text-[10px] text-white/30">{recentProjects.length} saved</span>
                </div>

                <div className="grid grid-cols-1 gap-2.5">
                  {recentProjects.map((p) => (
                    <button
                      key={p.path}
                      onClick={() => void selectProject(p.path)}
                      className="group flex items-center justify-between rounded-xl border border-white/10 bg-[#0E121B]/80 hover:bg-[#141A26] px-4 py-3 text-left transition-all hover:border-white/20 cursor-pointer"
                    >
                      <div className="min-w-0 pr-3">
                        <div className="flex items-center gap-2">
                          <span className="text-xs font-semibold text-white/90 group-hover:text-white truncate">
                            {getFolderName(p.path)}
                          </span>
                          {p.stats.language && (
                            <span
                              className={`rounded border px-1.5 py-0.2 font-mono text-[9px] font-bold uppercase ${getLanguageBadge(p.stats.language)}`}
                            >
                              {p.stats.language}
                            </span>
                          )}
                        </div>
                        <span className="font-mono text-[10px] text-white/40 truncate block mt-0.5" title={p.path}>
                          {p.path}
                        </span>
                      </div>

                      <div className="flex items-center gap-3 shrink-0">
                        <div className="text-right text-[10px] font-mono text-white/40 hidden sm:block">
                          <div>{p.stats.files || 0} files · {p.stats.symbols || 0} symbols</div>
                          <div className="flex items-center justify-end gap-1 text-[9px] text-white/30 mt-0.5">
                            <Clock size={10} />
                            {formatRelativeTime(p.last_opened)}
                          </div>
                        </div>
                        <FileCode2 size={15} className="text-white/20 group-hover:text-white/60 transition-colors" />
                      </div>
                    </button>
                  ))}
                </div>
              </div>
            )}

            {/* Sync Settings */}
            <div className="mt-8 pt-6 border-t border-white/10 flex items-center justify-between text-xs text-white/40 max-w-xl mx-auto w-full">
              <span>File Watcher Debounce:</span>
              <div className="flex items-center gap-1.5 font-mono text-[11px]">
                {[500, 1000, 2000, 5000].map((ms) => (
                  <button
                    key={ms}
                    onClick={() => void handleUpdateDebounce(ms)}
                    className={[
                      'rounded-md px-2 py-0.5 transition-all cursor-pointer border-0',
                      debounceMs === ms
                        ? 'bg-blue-600 text-white font-bold'
                        : 'bg-white/[0.04] text-white/50 hover:bg-white/10 hover:text-white/80',
                    ].join(' ')}
                  >
                    {ms >= 1000 ? `${ms / 1000}s` : `${ms}ms`}
                  </button>
                ))}
              </div>
            </div>
          </div>
        )}
      </main>
    </div>
  )
}

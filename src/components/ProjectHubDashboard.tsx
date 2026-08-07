import { useEffect, useState } from 'react'
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
  walking: 'Scanning workspace files…',
  parsing: 'Parsing source files',
  db_write: 'Building SQLite dependency graph…',
  complete: 'Finalizing index…',
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

  const getLanguageColor = (lang: string) => {
    switch (lang.toLowerCase()) {
      case 'rust':
        return 'text-orange-400 bg-orange-400/10 border-orange-400/20'
      case 'typescript':
      case 'javascript':
        return 'text-blue-400 bg-blue-400/10 border-blue-400/20'
      case 'python':
        return 'text-green-400 bg-green-400/10 border-green-400/20'
      case 'go':
        return 'text-cyan-400 bg-cyan-400/10 border-cyan-400/20'
      default:
        return 'text-white/50 bg-white/5 border-white/10'
    }
  }

  const getFolderName = (fullPath: string) => {
    const clean = fullPath.replace(/\\/g, '/')
    return clean.split('/').pop() || fullPath
  }

  return (
    <div className="flex min-h-screen flex-col bg-[#0D0F12] text-white font-sans selection:bg-active-purple/30 relative">
      {/* Background Decorative Grid */}
      <div className="absolute inset-0 bg-[linear-gradient(to_right,#1c232c_1px,transparent_1px),linear-gradient(to_bottom,#1c232c_1px,transparent_1px)] bg-[size:4rem_4rem] [mask-image:radial-gradient(ellipse_60%_50%_at_50%_0%,#000_70%,transparent_100%)] opacity-35 pointer-events-none" />

      <main className="relative z-10 flex flex-1 flex-col items-center justify-center p-8 max-w-4xl mx-auto w-full">
        {/* Header */}
        <div className="text-center mb-10 select-none">
          <div className="inline-flex items-center gap-2 rounded-full border border-active-purple/30 bg-active-purple/10 px-3 py-1 text-[11px] font-semibold text-active-purple uppercase tracking-wider mb-4 animate-pulse">
            <span className="h-1.5 w-1.5 rounded-full bg-active-purple" />
            SQLite Indexing Active
          </div>
          <h1 className="text-4xl font-bold tracking-tight bg-gradient-to-r from-white via-white to-active-purple/60 bg-clip-text text-transparent">
            Repo Graph Hub
          </h1>
          <p className="mt-3 text-sm text-white/50 max-w-md mx-auto leading-relaxed">
            Trace software structures, query call graphs, and simulate impact chains in sub-10ms SQLite syncs.
          </p>
        </div>

        {/* Real-time Indexing Progress HUD */}
        {isIndexing ? (
          status !== 'indexing' ? (
            <div className="flex flex-col items-center gap-4 py-12">
              <div className="h-8 w-8 animate-spin rounded-full border-2 border-active-purple border-t-transparent" />
              <div className="text-xs text-white/40 font-mono tracking-wider animate-pulse">
                LOADING GRAPH DATA...
              </div>
            </div>
          ) : (
            <div className="w-full max-w-md flex flex-col items-center gap-5 py-10 border border-white/5 bg-[#151c24]/30 rounded-xl px-8 backdrop-blur-xl shadow-2xl">
              <div className="relative h-12 w-12 shrink-0">
                <div className="absolute inset-0 rounded-full border-2 border-active-purple/15" />
                <div className="absolute inset-0 animate-spin rounded-full border-2 border-active-purple border-t-transparent" />
              </div>

              <div className="w-full flex flex-col gap-2">
                <div className="h-1.5 w-full overflow-hidden rounded-full bg-white/5 border border-white/5">
                  <div
                    className="h-full rounded-full bg-gradient-to-r from-active-purple to-fuchsia-400 transition-[width] duration-200 ease-out"
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

              <div className="text-xs text-white/60 font-mono text-center leading-relaxed">
                {indexProgress.filesTotal > 0
                  ? `${indexProgress.filesProcessed.toLocaleString()} / ${indexProgress.filesTotal.toLocaleString()} files — ${formatBytes(indexProgress.bytesProcessed)} / ${formatBytes(indexProgress.bytesTotal)}`
                  : 'Scanning workspace…'}
              </div>

              {indexProgress.etaSeconds !== null && indexProgress.phase === 'parsing' && (
                <div className="text-[11px] text-active-purple/80 font-mono tracking-wide">
                  Remaining: {formatEta(indexProgress.etaSeconds)}
                  {indexProgress.speedFilesPerSec > 0 && ` (~${Math.round(indexProgress.speedFilesPerSec)} files/sec)`}
                </div>
              )}
            </div>
          )
        ) : (
          <div className="w-full flex flex-col gap-6">
            {/* CTA Option */}
            <div className="flex flex-col items-center justify-center border border-white/5 bg-[#151c24]/30 rounded-xl p-8 text-center backdrop-blur transition-all duration-300 hover:border-active-purple/30 shadow-2xl relative overflow-hidden group">
              <div className="absolute top-0 right-0 h-[200%] w-[200%] translate-x-[50%] -translate-y-[50%] bg-[radial-gradient(circle_at_center,rgba(163,113,247,0.06)_0%,transparent_70%)] pointer-events-none group-hover:scale-110 transition-transform duration-500" />
              <button
                onClick={openProject}
                className="flex items-center gap-2.5 rounded-lg bg-active-purple px-6 py-3 text-sm font-semibold text-white shadow-lg hover:bg-active-purple/90 transition-all duration-200"
              >
                <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                </svg>
                Open Project Folder
              </button>
              <span className="text-[11px] text-white/30 font-mono mt-3 select-none">
                Select any folder to start indexing dependencies
              </span>
            </div>

            {/* Error Message */}
            {loadError && (
              <div className="border border-red-500/20 bg-red-950/20 text-red-400 rounded-lg p-4 text-xs font-mono text-center">
                Error: {loadError}
              </div>
            )}

            {/* Recent Projects */}
            {recentProjects.length > 0 && (
              <div className="flex flex-col gap-3">
                <h2 className="text-xs font-semibold uppercase tracking-widest text-white/40 select-none">
                  Recent Workspaces
                </h2>
                <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                  {recentProjects.map((proj) => (
                    <button
                      key={proj.path}
                      onClick={() => selectProject(proj.path)}
                      className="flex flex-col text-left border border-white/5 bg-[#151c24]/20 hover:bg-[#151c24]/50 rounded-lg p-4 transition-all duration-200 hover:border-active-purple/40 hover:shadow-xl group"
                    >
                      <div className="flex justify-between items-start w-full">
                        <span className="font-semibold text-sm group-hover:text-active-purple transition-colors truncate max-w-[70%]">
                          {getFolderName(proj.path)}
                        </span>
                        <span className="text-[10px] text-white/30 font-mono">
                          {formatRelativeTime(proj.last_opened)}
                        </span>
                      </div>
                      <span className="text-[11px] text-white/40 truncate w-full mt-1 font-mono">
                        {proj.path}
                      </span>
                      
                      {/* Stats */}
                      <div className="flex gap-2.5 mt-4">
                        <span className="flex items-center gap-1 text-[10px] text-white/50 bg-white/5 border border-white/5 px-2 py-0.5 rounded font-mono">
                          Files: <strong className="text-white/80">{proj.stats.files}</strong>
                        </span>
                        <span className="flex items-center gap-1 text-[10px] text-white/50 bg-white/5 border border-white/5 px-2 py-0.5 rounded font-mono">
                          Symbols: <strong className="text-white/80">{proj.stats.symbols}</strong>
                        </span>
                        {proj.stats.language && (
                          <span className={`text-[10px] border px-2 py-0.5 rounded capitalize font-mono ${getLanguageColor(proj.stats.language)}`}>
                            {proj.stats.language}
                          </span>
                        )}
                      </div>
                    </button>
                  ))}
                </div>
              </div>
            )}

            {/* Quick Settings Configuration */}
            <div className="mt-4 border border-white/5 bg-[#151c24]/10 rounded-lg p-5">
              <h3 className="text-xs font-semibold uppercase tracking-wider text-white/60 mb-4 select-none">
                File Watcher Preferences
              </h3>
              <div className="flex flex-col gap-2">
                <div className="flex justify-between text-xs font-mono">
                  <span className="text-white/50">Debounce Write Latency:</span>
                  <span className="text-active-purple font-semibold">{debounceMs} ms</span>
                </div>
                <input
                  type="range"
                  min="200"
                  max="10000"
                  step="100"
                  value={debounceMs}
                  onChange={(e) => handleUpdateDebounce(Number(e.target.value))}
                  className="w-full h-1 bg-white/10 rounded-lg appearance-none cursor-pointer accent-active-purple"
                />
                <span className="text-[10px] text-white/30 font-mono select-none">
                  Debounces background file parsing before writing updates dynamically to SQLite.
                </span>
              </div>
            </div>
          </div>
        )}
      </main>
    </div>
  )
}

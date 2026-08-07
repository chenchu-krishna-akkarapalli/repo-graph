import { useEffect, useRef, useState } from 'react'
import { HelpCircle, Home, Search, X } from 'lucide-react'
import { useGraphStore } from '../store'
import { tauriInvoke } from '../lib/loadGraph'
import type { SymbolSearchResult, SyncStatus } from '../types'

const STATUS_META: Record<SyncStatus, { label: string; dotClass: string; pulse: boolean }> = {
  indexing: { label: 'Indexing…', dotClass: 'bg-amber-400 shadow-[0_0_8px_rgba(245,158,11,0.6)]', pulse: true },
  synced: { label: 'Watching (Synced)', dotClass: 'bg-emerald-400 shadow-[0_0_8px_rgba(52,211,153,0.6)]', pulse: false },
  stale: { label: 'Offline / Stale', dotClass: 'bg-rose-500 shadow-[0_0_8px_rgba(244,63,94,0.6)]', pulse: true },
  updated: { label: 'Watching (Synced - Updated)', dotClass: 'bg-emerald-400 shadow-[0_0_10px_rgba(52,211,153,0.8)]', pulse: true },
}

const FILTERS: { key: string; label: string; activeClass: string }[] = [
  { key: 'javascript', label: 'JS/TS', activeClass: 'border-blue-500/50 bg-blue-500/15 text-blue-400 shadow-[0_0_12px_rgba(59,130,246,0.25)]' },
  { key: 'python', label: 'PY', activeClass: 'border-emerald-500/50 bg-emerald-500/15 text-emerald-400 shadow-[0_0_12px_rgba(16,185,129,0.25)]' },
  { key: 'rust', label: 'RS', activeClass: 'border-amber-500/50 bg-amber-500/15 text-amber-400 shadow-[0_0_12px_rgba(245,158,11,0.25)]' },
]

function ProjectRootBadge() {
  const root = useGraphStore((s) => s.activeProjectRoot)
  if (!root) return null
  const name = root.replace(/\\/g, '/').split('/').filter(Boolean).pop() ?? root
  return (
    <span
      className="max-w-56 truncate rounded-full border border-white/10 bg-white/[0.04] px-2.5 py-0.5 font-mono text-[10px] text-white/50"
      title={root}
    >
      {name}
    </span>
  )
}

export default function TopToolbar() {
  const status = useGraphStore((s) => s.status)
  const searchQuery = useGraphStore((s) => s.searchQuery)
  const setSearchQuery = useGraphStore((s) => s.setSearchQuery)
  const matchCount = useGraphStore((s) => s.searchMatches.size)
  const languageFilters = useGraphStore((s) => s.languageFilters)
  const toggleLanguageFilter = useGraphStore((s) => s.toggleLanguageFilter)
  const goHome = useGraphStore((s) => s.goHome)
  const inputRef = useRef<HTMLInputElement>(null)

  const [symbolResults, setSymbolResults] = useState<SymbolSearchResult[]>([])
  const [isSearchingSymbols, setIsSearchingSymbols] = useState(false)
  const [showDropdown, setShowDropdown] = useState(false)
  const containerRef = useRef<HTMLDivElement>(null)

  const isSymbolSearch = searchQuery.startsWith('sym:')
  const symbolQuery = isSymbolSearch ? searchQuery.substring(4) : ''

  useEffect(() => {
    if (isSymbolSearch && symbolQuery.trim().length > 0) {
      const invoke = tauriInvoke()
      if (invoke) {
        // Debounced IPC search: the pending flag has to be set before the
        // request starts, which is inherently an effect.
        // eslint-disable-next-line react-hooks/set-state-in-effect
        setIsSearchingSymbols(true)
        setShowDropdown(true)
        const delayDebounce = setTimeout(() => {
          invoke('search_symbols', { query: symbolQuery.trim() })
            .then((res) => {
              setSymbolResults((res as SymbolSearchResult[]) ?? [])
            })
            .catch((err) => {
              console.error(err)
              setSymbolResults([])
            })
            .finally(() => {
              setIsSearchingSymbols(false)
            })
        }, 150)
        return () => clearTimeout(delayDebounce)
      }
    } else {
      setSymbolResults([])
      setShowDropdown(false)
    }
  }, [searchQuery, isSymbolSearch, symbolQuery])

  useEffect(() => {
    const onClickOutside = (e: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setShowDropdown(false)
      }
    }
    document.addEventListener('mousedown', onClickOutside)
    return () => document.removeEventListener('mousedown', onClickOutside)
  }, [])

  // Ctrl+K / Cmd+K focuses the fuzzy search (Jakob's Law shortcut).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault()
        inputRef.current?.focus()
        inputRef.current?.select()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  const meta = STATUS_META[status]

  return (
    <header className="flex h-12 shrink-0 items-center justify-between gap-4 border-b border-white/10 bg-[#0B0D12]/90 px-4 backdrop-blur-md z-40">
      <div className="flex items-center gap-3">
        <div className="bg-gradient-to-r from-violet-400 to-cyan-400 bg-clip-text text-transparent font-bold text-sm tracking-wide">
          Repo Graph
        </div>
        <button
          onClick={goHome}
          title="Back to Project Hub"
          aria-label="Back to Project Hub"
          className="flex h-7 w-7 items-center justify-center rounded-lg border border-white/10 bg-white/[0.03] text-white/60 transition-colors hover:border-violet-500/50 hover:bg-violet-500/10 hover:text-white cursor-pointer"
        >
          <Home size={15} />
        </button>
        <ProjectRootBadge />
      </div>

      <div ref={containerRef} className="relative w-80 max-w-[42vw]">
        <div className="relative flex items-center rounded-lg border border-white/10 bg-[#141822] backdrop-blur-sm transition-all hover:border-white/20 focus-within:border-violet-500 focus-within:ring-1 focus-within:ring-violet-500/30">
          <Search size={14} className="pointer-events-none absolute left-3 text-white/40" />
          <input
            ref={inputRef}
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            onFocus={() => {
              if (isSymbolSearch) setShowDropdown(true)
            }}
            onKeyDown={(e) => {
              if (e.key === 'Escape') {
                setSearchQuery('')
                setShowDropdown(false)
                e.currentTarget.blur()
              }
            }}
            placeholder="Search files or 'sym:name'…"
            className="h-8 w-full bg-transparent pl-9 pr-14 text-xs text-white/90 placeholder:text-white/40 focus:outline-none"
          />
          {searchQuery !== '' ? (
            <button
              onClick={() => {
                setSearchQuery('')
                setShowDropdown(false)
              }}
              className="absolute right-2 rounded-md p-1 text-white/40 hover:bg-white/10 hover:text-white/80 cursor-pointer border-0"
              aria-label="Clear search"
            >
              <X size={13} />
            </button>
          ) : (
            <kbd className="pointer-events-none absolute right-2.5 flex items-center gap-0.5 rounded border border-white/10 bg-white/[0.06] px-1.5 py-0.5 font-mono text-[10px] text-white/40">
              <span>⌘</span>K
            </kbd>
          )}
        </div>

        {showDropdown && (symbolResults.length > 0 || isSearchingSymbols) && (
          <div className="absolute left-0 right-0 top-full z-50 mt-2 max-h-64 overflow-y-auto rounded-xl border border-white/10 bg-[#0F1218]/95 p-2 shadow-2xl backdrop-blur-md">
            {isSearchingSymbols && (
              <div className="px-3 py-2 text-xs text-white/40">Searching symbols...</div>
            )}
            {!isSearchingSymbols && symbolResults.length === 0 && (
              <div className="px-3 py-2 text-xs text-white/40">No symbols found</div>
            )}
            {!isSearchingSymbols && symbolResults.map((sym) => (
              <button
                key={`${sym.file_path}#${sym.name}`}
                onClick={() => {
                  const expanded = useGraphStore.getState().expandedFiles
                  if (!expanded.has(sym.file_path)) {
                    useGraphStore.getState().toggleExpandFile(sym.file_path)
                  }
                  useGraphStore.getState().focusNode(sym.file_path)
                  useGraphStore.getState().setSelectedSymbol({ path: sym.file_path, name: sym.name })
                  setSearchQuery('')
                  setShowDropdown(false)
                }}
                className="flex w-full flex-col gap-0.5 rounded-lg px-3 py-2 text-left hover:bg-white/[0.06] transition-colors border-0 bg-transparent cursor-pointer"
              >
                <span className="text-xs font-semibold text-violet-300">{sym.name}</span>
                <span className="font-mono text-[10px] text-white/40 truncate">{sym.file_path}</span>
              </button>
            ))}
          </div>
        )}
      </div>

      <div className="flex items-center gap-3">
        {!isSymbolSearch && searchQuery.trim() !== '' && (
          <span className="font-mono text-[10px] text-violet-300 bg-violet-500/10 border border-violet-500/30 px-2.5 py-1 rounded-full shrink-0">
            {matchCount} matches
          </span>
        )}

        <div className="flex items-center gap-1.5">
          {FILTERS.map((f) => {
            const active = languageFilters.has(f.key)
            return (
              <button
                key={f.key}
                onClick={() => toggleLanguageFilter(f.key)}
                aria-pressed={active}
                className={[
                  'h-7 rounded-full border px-3 font-mono text-xs transition-all duration-200 cursor-pointer',
                  active
                    ? f.activeClass
                    : 'border-white/10 bg-white/[0.02] text-white/40 hover:border-white/20 hover:text-white/70',
                ].join(' ')}
              >
                {f.label}
              </button>
            )
          })}
        </div>

        {/* Re-open the CEPA guide on demand. A window event keeps the modal's
            state owned by App.tsx — the toolbar does not need to know it exists. */}
        <button
          onClick={() => window.dispatchEvent(new Event('repograph:open-cepa-guide'))}
          title="CEPA Guide — cut agent token usage with the 3-step discovery sequence"
          aria-label="Open CEPA Guide"
          className="flex h-7 items-center gap-1.5 rounded-full border border-white/10 bg-white/[0.02] px-3 text-xs font-medium text-white/50 transition-all hover:border-violet-500/50 hover:bg-violet-500/10 hover:text-white/90 cursor-pointer"
        >
          <HelpCircle size={15} />
          CEPA Guide
        </button>

        {/* Status pill indicator */}
        <div className="flex h-7 items-center gap-2 rounded-full border border-white/10 bg-[#090A0F]/60 px-3 backdrop-blur-sm">
          <span
            className={`h-2 w-2 rounded-full ${meta.dotClass} ${meta.pulse ? 'status-dot-pulse' : ''}`}
          />
          <span className="text-xs font-medium text-white/70">{meta.label}</span>
        </div>
      </div>
    </header>
  )
}

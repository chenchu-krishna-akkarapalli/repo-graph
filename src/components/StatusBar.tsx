import { useGraphStore } from '../store'

/** 28px telemetry strip: counts, cache load latency, search latency. */
export default function StatusBar() {
  const graph = useGraphStore((s) => s.graph)
  const indexLatencyMs = useGraphStore((s) => s.indexLatencyMs)
  const searchLatencyMs = useGraphStore((s) => s.searchLatencyMs)
  const searchActive = useGraphStore((s) => s.searchQuery.trim() !== '')
  const impactCount = useGraphStore((s) => (s.impactSource ? s.impactSet.size : null))
  const status = useGraphStore((s) => s.status)

  return (
    <footer className="flex h-7 shrink-0 items-center gap-4 border-t border-accent-border bg-panel-bg px-3 font-mono text-[11px] text-white/45">
      <span>
        {graph ? `${graph.nodes.length.toLocaleString()} files` : 'loading…'}
        {graph ? ` | ${graph.edges.length.toLocaleString()} edges` : ''}
      </span>
      <span>Index load: {indexLatencyMs} ms</span>
      {searchActive && <span>Search: {searchLatencyMs} ms</span>}
      {impactCount !== null && (
        <span className="text-impact-red">Impact: {impactCount} affected</span>
      )}
      {graph && graph.warnings.length > 0 && (
        <span className="text-impact-yellow">{graph.warnings.length} map warnings</span>
      )}
      <span className="ml-auto">
        {status === 'synced' ? 'watcher: live' : status === 'indexing' ? 'watcher: indexing' : 'watcher: offline'}
      </span>
    </footer>
  )
}

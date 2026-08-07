import { useEffect, useRef, useState } from 'react'
import { useGraphStore, type RawIndexProgressPayload } from './store'
import { showDesktopWindow, tauriListen } from './lib/loadGraph'
import TopToolbar from './components/TopToolbar'
import LeftSidebar from './components/LeftSidebar'
import GraphCanvas from './components/GraphCanvas'
import DetailSidebar from './components/DetailSidebar'
import StatusBar from './components/StatusBar'
import ProjectHubDashboard from './components/ProjectHubDashboard'
import CEPAUserGuideModal from './components/CEPAUserGuideModal'
import { isCepaDismissed } from './lib/cepaPreferences'

/** `agent_query_event` payload emitted by the Rust telemetry hook. */
interface AgentQueryPayload {
  symbol?: string | null
  path?: string | null
  action?: string
}

export default function App() {
  const load = useGraphStore((s) => s.load)
  const activeProjectRoot = useGraphStore((s) => s.activeProjectRoot)
  const status = useGraphStore((s) => s.status)
  const [showCEPATour, setShowCEPATour] = useState(false)
  /** Projects already offered the guide this session — one prompt, not one per re-index. */
  const offeredFor = useRef<string | null>(null)

  // Auto-open once a project has finished indexing.
  //
  // Note this keys on 'synced' as well as 'updated': `openProject` and
  // `selectProject` both settle on 'synced', while 'updated' is only ever set
  // by the `graph_updated` watcher event below. Waiting for 'updated' alone
  // would mean the guide never appeared on a fresh project open — only after
  // the user happened to edit a file.
  useEffect(() => {
    if (!activeProjectRoot) {
      offeredFor.current = null // back at the hub; a later open may offer again
      return
    }
    if (status !== 'synced' && status !== 'updated') return
    if (offeredFor.current === activeProjectRoot) return
    offeredFor.current = activeProjectRoot
    // Reading localStorage is an external-system sync, not derived state:
    // there is nothing to compute this from during render.
    // eslint-disable-next-line react-hooks/set-state-in-effect
    if (!isCepaDismissed()) setShowCEPATour(true)
  }, [activeProjectRoot, status])

  // Toolbar "CEPA Guide" button re-opens it on demand.
  useEffect(() => {
    const onOpen = () => setShowCEPATour(true)
    window.addEventListener('repograph:open-cepa-guide', onOpen)
    return () => window.removeEventListener('repograph:open-cepa-guide', onOpen)
  }, [])

  useEffect(() => {
    showDesktopWindow()
    void load()
  }, [load])

  useEffect(() => {
    const unlistens: (() => void)[] = []

    const listenToEvents = async () => {
      const listen = tauriListen()
      if (!listen) return

      try {
        const unsubscribeGraph = await listen<void>('graph_updated', () => {
          useGraphStore.setState({ status: 'updated' })
          void load()
          setTimeout(() => {
            useGraphStore.setState((state) => {
              if (state.status === 'updated') {
                return { status: 'synced' }
              }
              return {}
            })
          }, 2000)
        })
        unlistens.push(unsubscribeGraph)

        const unsubscribeProgress = await listen<RawIndexProgressPayload>(
          'index_progress',
          (event) => useGraphStore.getState().setIndexProgress(event.payload),
        )
        unlistens.push(unsubscribeProgress)

        const unsubscribeTelemetry = await listen<AgentQueryPayload>(
          'agent_query_event',
          (event) => {
            const { symbol, path, action } = event.payload ?? {}
            useGraphStore.getState().addAgentActivity({
              id: Math.random().toString(36).substring(7),
              // Null, not '': one of the two is genuinely absent depending on
              // how the agent queried, and `agentActivityTargets` branches on it.
              symbol: symbol || null,
              path: path || null,
              action: action || '',
              timestamp: Date.now(),
            })
          },
        )
        unlistens.push(unsubscribeTelemetry)
      } catch (err) {
        console.error('Failed to subscribe to Tauri events:', err)
      }
    }

    void listenToEvents()

    return () => {
      for (const u of unlistens) {
        u()
      }
    }
  }, [load])

  if (activeProjectRoot === null) {
    return <ProjectHubDashboard />
  }

  return (
    <div className="flex h-full flex-col">
      <TopToolbar />
      <main className="flex min-h-0 flex-1">
        <LeftSidebar />
        <section className="min-w-0 flex-1">
          <GraphCanvas />
        </section>
        <DetailSidebar />
      </main>
      <StatusBar />
      <CEPAUserGuideModal open={showCEPATour} onClose={() => setShowCEPATour(false)} />
    </div>
  )
}

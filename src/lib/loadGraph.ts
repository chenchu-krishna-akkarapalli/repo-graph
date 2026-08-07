import type { RepoGraph } from '../types'

/** Tauri globals (`withGlobalTauri`): v1 exposes `tauri.invoke` and
 *  `window.appWindow`; v2 exposes `core.invoke`. Support both shapes. */
type Invoke = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>

/** One `emit_all` delivery from the Rust side. */
export interface TauriEvent<T = unknown> {
  event: string
  payload: T
}

/** Returns an unsubscribe function, per Tauri's event API. */
type Listen = <T>(
  event: string,
  handler: (event: TauriEvent<T>) => void,
) => Promise<() => void>

declare global {
  interface Window {
    __TAURI__?: {
      core?: { invoke: Invoke }
      tauri?: { invoke: Invoke }
      invoke?: Invoke
      event?: { listen?: Listen }
      window?: { appWindow?: { show: () => Promise<void> } }
    }
  }
}

export function tauriInvoke(): Invoke | null {
  const t = window.__TAURI__
  return t?.core?.invoke ?? t?.tauri?.invoke ?? t?.invoke ?? null
}

/** `null` outside the packaged app, where there is no event bus. */
export function tauriListen(): Listen | null {
  return window.__TAURI__?.event?.listen ?? null
}

/** Reveal the (initially hidden) native window once the dark UI is ready —
 *  prevents the white flash before first paint. No-op in the browser. */
export function showDesktopWindow(): void {
  void window.__TAURI__?.window?.appWindow?.show()
}

/**
 * Tauri bridge with dev fallback: inside the packaged app the graph cache
 * comes over IPC (`read_graph` command); under `vite dev` in a plain
 * browser the config's middleware serves the same file at `/api/graph`.
 */
export async function loadGraph(): Promise<{ graph: RepoGraph; latencyMs: number }> {
  const started = performance.now()
  let graph: RepoGraph
  const invoke = tauriInvoke()
  if (invoke) {
    graph = (await invoke('read_graph')) as RepoGraph
  } else {
    const res = await fetch('/api/graph')
    if (!res.ok) throw new Error(`graph fetch failed: ${res.status}`)
    graph = (await res.json()) as RepoGraph
  }
  return { graph, latencyMs: Math.round(performance.now() - started) }
}

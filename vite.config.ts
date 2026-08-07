import { readFile } from 'node:fs/promises'
import { resolve } from 'node:path'
import { defineConfig, type Plugin } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

/**
 * Dev bridge: serves the walker's cache (`.repograph/graph.json`) at
 * `/api/graph`. In the packaged Tauri app the same payload comes over IPC
 * (`invoke("read_graph")`) — see src/lib/loadGraph.ts.
 */
function repoGraphBridge(): Plugin {
  return {
    name: 'repo-graph-bridge',
    configureServer(server) {
      server.middlewares.use('/api/graph', async (_req, res) => {
        try {
          const raw = await readFile(resolve(__dirname, '.repograph/graph.json'), 'utf-8')
          res.setHeader('Content-Type', 'application/json')
          res.end(raw)
        } catch {
          res.statusCode = 404
          res.end(JSON.stringify({ error: 'graph_cache_missing — run: mcp_server index .' }))
        }
      })
    },
  }
}

export default defineConfig({
  plugins: [react(), tailwindcss(), repoGraphBridge()],
  server: {
    port: Number(process.env.PORT) || 5173,
    strictPort: Boolean(process.env.PORT),
    watch: {
      ignored: [
        '**/.repograph/**',
        '**/active_project.json',
        '**/target/**',
        '**/src-tauri/target/**',
      ],
    },
  },
})

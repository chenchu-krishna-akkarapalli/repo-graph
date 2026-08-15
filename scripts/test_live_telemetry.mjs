import { spawn } from 'child_process'
import { resolve } from 'path'

// We can read .repograph/graph.db using sqlite3 or via node test script
const binaryPath = resolve('src-tauri/target/debug/mcp_server.exe')
const projectRoot = resolve('.')

console.log('Testing live Agent MCP Telemetry across all tools...')

const proc = spawn(binaryPath, [projectRoot], {
  env: {
    ...process.env,
    REPOGRAPH_MCP_TOOLS: 'explore,files,node,search,impact,callers,callees,status',
  },
  stdio: ['pipe', 'pipe', 'inherit'],
})

let buffer = ''
const responses = new Map()

proc.stdout.on('data', (data) => {
  buffer += data.toString()
  const lines = buffer.split('\n')
  buffer = lines.pop() || ''
  for (const line of lines) {
    if (line.trim()) {
      try {
        const json = JSON.parse(line.trim())
        if (json.id !== undefined) {
          responses.set(json.id, json)
        }
      } catch (err) {
        console.error('Failed to parse line:', line, err)
      }
    }
  }
})

let nextId = 1
function send(method, params) {
  const id = nextId++
  const msg = { jsonrpc: '2.0', id, method, params }
  proc.stdin.write(JSON.stringify(msg) + '\n')
  return new Promise((resolve, reject) => {
    const start = Date.now()
    const check = () => {
      if (responses.has(id)) {
        resolve(responses.get(id))
      } else if (Date.now() - start > 10000) {
        reject(new Error(`Timeout waiting for response to id ${id} (${method})`))
      } else {
        setTimeout(check, 10)
      }
    }
    check()
  })
}

async function run() {
  await send('initialize', { protocolVersion: '2025-06-18' })

  // Exercise every tool
  console.log('1. Calling repograph_files...')
  await send('tools/call', { name: 'repograph_files', arguments: { top_k: 3 } })

  console.log('2. Calling repograph_node (file)...')
  await send('tools/call', { name: 'repograph_node', arguments: { path: 'src/store.ts' } })

  console.log('3. Calling repograph_node (symbol)...')
  await send('tools/call', { name: 'repograph_node', arguments: { path: 'src/store.ts', symbol: 'useGraphStore' } })

  console.log('4. Calling repograph_search...')
  await send('tools/call', { name: 'repograph_search', arguments: { query: 'savePersistedGraph' } })

  console.log('5. Calling repograph_explore...')
  await send('tools/call', { name: 'repograph_explore', arguments: { symbols: ['useGraphStore'] } })

  console.log('6. Calling repograph_callers...')
  await send('tools/call', { name: 'repograph_callers', arguments: { symbol: 'useGraphStore' } })

  console.log('7. Calling repograph_callees...')
  await send('tools/call', { name: 'repograph_callees', arguments: { symbol: 'useGraphStore' } })

  console.log('8. Calling repograph_impact...')
  await send('tools/call', { name: 'repograph_impact', arguments: { path: 'src/store.ts' } })

  console.log('9. Calling repograph_status...')
  const statusRes = await send('tools/call', { name: 'repograph_status', arguments: {} })
  console.log('\nFinal Status Output:\n' + statusRes.result?.content?.[0]?.text)

  proc.stdin.end()
  proc.kill()
  console.log('\n✓ Live Telemetry pipeline successfully exercised for all 8 tools.')
}

run()

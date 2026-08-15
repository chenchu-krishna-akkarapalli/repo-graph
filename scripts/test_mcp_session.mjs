import { spawn } from 'child_process'
import { resolve } from 'path'

const binaryPath = resolve('src-tauri/target/debug/mcp_server.exe')
const projectRoot = resolve('.')

console.log(`[Testing MCP Server] Binary: ${binaryPath}`)
console.log(`[Testing MCP Server] Root: ${projectRoot}`)

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
        setTimeout(check, 20)
      }
    }
    check()
  })
}

async function runTests() {
  const results = { passed: 0, failed: 0, gaps: [] }

  function assert(condition, message) {
    if (condition) {
      results.passed++
      console.log(`  ✓ ${message}`)
    } else {
      results.failed++
      results.gaps.push(message)
      console.error(`  ✗ FAIL: ${message}`)
    }
  }

  try {
    console.log('\n--- 1. Initialize & Session Persistence Handshake ---')
    const initRes = await send('initialize', { protocolVersion: '2025-06-18' })
    assert(initRes.result?.session?.connected === true, 'Session is marked connected in initialize')
    assert(initRes.result?.session?.sessionActive === true, 'Session is marked active in initialize')
    assert(initRes.result?.session?.persistent === true, 'Session is marked persistent')
    const sessionId = initRes.result?.session?.sessionId
    assert(Boolean(sessionId), `Session ID established: ${sessionId}`)

    console.log('\n--- 2. Tools Discovery & All 8 Tools Available ---')
    const listRes = await send('tools/list', {})
    const toolNames = (listRes.result?.tools || []).map((t) => t.name)
    const requiredTools = [
      'repograph_status',
      'repograph_files',
      'repograph_node',
      'repograph_callers',
      'repograph_callees',
      'repograph_impact',
      'repograph_search',
      'repograph_explore',
    ]
    for (const tool of requiredTools) {
      assert(toolNames.includes(tool), `Tool '${tool}' is exposed in tools/list`)
    }

    console.log('\n--- 3. Keepalive / Heartbeat Ping ---')
    const pingRes1 = await send('ping', {})
    assert(pingRes1.result?.status === 'ok', 'Ping returns status: ok')
    assert(pingRes1.result?.connected === true, 'Ping returns connected: true')
    assert(pingRes1.result?.session_active === true, 'Ping returns session_active: true')
    assert(pingRes1.result?.session_id === sessionId, 'Ping preserves established session ID')
    assert(pingRes1.result?.heartbeats === 1, 'Heartbeat count starts at 1')

    const pingRes2 = await send('ping', {})
    assert(pingRes2.result?.heartbeats === 2, 'Consecutive ping increments heartbeat count to 2')

    console.log('\n--- 4. Tool Execution: repograph_status ---')
    const statusRes = await send('tools/call', { name: 'repograph_status', arguments: {} })
    const statusText = statusRes.result?.content?.[0]?.text || ''
    assert(statusText.includes('connected: true'), 'repograph_status reports connected: true')
    assert(statusText.includes('session_active: true'), 'repograph_status reports session_active: true')
    assert(statusText.includes(sessionId), 'repograph_status reports active session ID')

    console.log('\n--- 5. Tool Execution: repograph_files (Centrality Ranking) ---')
    const filesRes = await send('tools/call', { name: 'repograph_files', arguments: { top_k: 5 } })
    const filesText = filesRes.result?.content?.[0]?.text || ''
    assert(filesRes.result?.isError === false, 'repograph_files succeeds without error')
    assert(filesText.includes('Project Architecture Map'), 'repograph_files outputs Architecture Map')

    console.log('\n--- 6. Tool Execution: repograph_node ---')
    const nodeRes = await send('tools/call', { name: 'repograph_node', arguments: { path: 'src/lib/graphCache.ts' } })
    const nodeText = nodeRes.result?.content?.[0]?.text || ''
    assert(nodeRes.result?.isError === false, 'repograph_node succeeds for src/lib/graphCache.ts')
    assert(nodeText.includes('savePersistedGraph'), 'repograph_node returns file content')

    console.log('\n--- 7. Tool Execution: repograph_search ---')
    const searchRes = await send('tools/call', { name: 'repograph_search', arguments: { query: 'useGraphStore' } })
    const searchText = searchRes.result?.content?.[0]?.text || ''
    assert(searchRes.result?.isError === false, 'repograph_search succeeds')
    assert(searchText.length > 0, 'repograph_search returns search matches')

    console.log('\n--- 8. Tool Execution: repograph_explore ---')
    const exploreRes = await send('tools/call', { name: 'repograph_explore', arguments: { symbols: ['useGraphStore'] } })
    const exploreText = exploreRes.result?.content?.[0]?.text || ''
    assert(exploreRes.result?.isError === false, 'repograph_explore succeeds')
    assert(exploreText.includes('useGraphStore'), 'repograph_explore returns symbol definition')

    console.log('\n--- 9. Tool Execution: repograph_callers & callees ---')
    const callersRes = await send('tools/call', { name: 'repograph_callers', arguments: { symbol: 'useGraphStore' } })
    assert(callersRes.result?.isError === false, 'repograph_callers executes successfully')
    const calleesRes = await send('tools/call', { name: 'repograph_callees', arguments: { symbol: 'useGraphStore' } })
    assert(calleesRes.result?.isError === false, 'repograph_callees executes successfully')

    console.log('\n--- 10. Tool Execution: repograph_impact ---')
    const impactRes = await send('tools/call', { name: 'repograph_impact', arguments: { path: 'src/store.ts' } })
    assert(impactRes.result?.isError === false, 'repograph_impact executes successfully')

    console.log('\n--- 11. Multi-turn Session Counter Verification ---')
    const finalStatus = await send('tools/call', { name: 'repograph_status', arguments: {} })
    const finalStatusText = finalStatus.result?.content?.[0]?.text || ''
    assert(finalStatusText.includes('tool queries'), 'Multi-turn query counter tracked in session status')
    assert(finalStatusText.includes(sessionId), 'Session ID persisted across all 11 turns')

  } catch (err) {
    console.error('Test execution error:', err)
    results.failed++
    results.gaps.push(err.message)
  } finally {
    proc.stdin.end()
    proc.kill()
  }

  console.log('\n=============================================')
  console.log(`RESULTS: Passed: ${results.passed}, Failed: ${results.failed}`)
  if (results.gaps.length > 0) {
    console.log('GAPS FOUND:')
    for (const gap of results.gaps) {
      console.log(` - ${gap}`)
    }
  } else {
    console.log('NO GAPS FOUND: All tests and MCP tools operating cleanly with persistent session.')
  }
  console.log('=============================================\n')

  process.exit(results.failed > 0 ? 1 : 0)
}

runTests()

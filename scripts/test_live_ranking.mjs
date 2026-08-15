import { spawn } from 'child_process'
import { resolve } from 'path'

const binaryPath = resolve('src-tauri/target/debug/mcp_server.exe')
const projectRoot = resolve('.')

console.log('=================================================================')
console.log('LIVE MCP TEST: RANKING STRATEGY & MANIFEST OPTIMIZATION ANALYSIS')
console.log('=================================================================\n')

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

async function testRankingStrategy() {
  try {
    await send('initialize', { protocolVersion: '2025-06-18' })

    // 1. Full Manifest Ranking
    console.log('[Test 1] Full Project Ranking Manifest...')
    const fullRes = await send('tools/call', { name: 'repograph_files', arguments: {} })
    const fullText = fullRes.result?.content?.[0]?.text || ''
    const fullLines = fullText.split('\n')
    console.log(`  📊 Total Manifest Size: ${fullText.length} characters (${fullLines.length} lines)`)

    // 2. Top-K Filtered Ranking (top_k = 5)
    console.log('\n[Test 2] Top-K Ranking Slicing (top_k: 5)...')
    const top5Res = await send('tools/call', { name: 'repograph_files', arguments: { top_k: 5 } })
    const top5Text = top5Res.result?.content?.[0]?.text || ''
    console.log('Top 5 Ranked Files Output Preview:\n' + top5Text.slice(0, 800) + '...\n')

    // 3. Min Rank Threshold Filter (min_rank = 0.3)
    console.log('[Test 3] Minimum Rank Threshold (min_rank: 0.3)...')
    const minRankRes = await send('tools/call', { name: 'repograph_files', arguments: { min_rank: 0.3 } })
    const minRankText = minRankRes.result?.content?.[0]?.text || ''
    const minRankLines = minRankText.split('\n').filter((l) => l.trim().startsWith('- ') || l.trim().startsWith('`'))
    console.log(`  📊 High-Centrality Filter (score >= 0.3) returned ${minRankLines.length} files.`)

    // 4. Scoped Subsystem Ranking
    console.log('\n[Test 4] Scoped Subsystem Ranking (scope: "src-tauri/src")...')
    const scopeRes = await send('tools/call', { name: 'repograph_files', arguments: { scope: 'src-tauri/src', top_k: 5 } })
    const scopeText = scopeRes.result?.content?.[0]?.text || ''
    console.log('Scoped Top 5 Backend Files:\n' + scopeText.slice(0, 600) + '...\n')

    // 5. Token Savings Ratio Calculation
    const fullTokenEst = Math.round(fullText.length / 3.8)
    const top5TokenEst = Math.round(top5Text.length / 3.8)
    const tokenSavingsPct = (((fullTokenEst - top5TokenEst) / fullTokenEst) * 100).toFixed(1)
    console.log('-----------------------------------------------------------------')
    console.log(`TOKEN BUDGET OPTIMIZATION:`)
    console.log(`  Full Manifest: ~${fullTokenEst} tokens`)
    console.log(`  Top-5 Ranked:  ~${top5TokenEst} tokens`)
    console.log(`  Context Reduction: ${tokenSavingsPct}% token savings achieved`)
    console.log('-----------------------------------------------------------------')

  } catch (err) {
    console.error('Error during ranking test:', err)
  } finally {
    proc.stdin.end()
    proc.kill()
  }
}

testRankingStrategy()

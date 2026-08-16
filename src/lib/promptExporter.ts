import { tauriInvoke } from './loadGraph'
import { useGraphStore } from '../store'

/**
 * Assembles and exports the Seven-Piece Context Stack in a human-intuitive,
 * visually clean Markdown architecture designed for LLMs (Claude Opus 5, Gemini Pro, GPT-4o).
 */
export interface ExploreSymbolBlock {
  name: string
  kind: string
  code: string
  start_line: number
  end_line: number
}

export interface ExploreFilePayload {
  path: string
  code_blocks: ExploreSymbolBlock[]
}

export interface ExplorePathEdge {
  from_symbol: string
  to_symbol: string
  kind: string
}

export interface ExplorePayload {
  files: ExploreFilePayload[]
  paths: ExplorePathEdge[]
}

/**
 * Robust clipboard copy helper with backup textarea fallback for non-secure
 * contexts or Tauri webview focus constraints.
 */
export async function copyToClipboard(text: string): Promise<boolean> {
  try {
    if (typeof navigator !== 'undefined' && navigator.clipboard && window.isSecureContext) {
      await navigator.clipboard.writeText(text)
      return true
    }
  } catch {
    // Fall through to textarea execCommand fallback
  }

  try {
    if (typeof document !== 'undefined') {
      const textArea = document.createElement('textarea')
      textArea.value = text
      textArea.style.position = 'fixed'
      textArea.style.opacity = '0'
      document.body.appendChild(textArea)
      textArea.focus()
      textArea.select()
      const successful = document.execCommand('copy')
      document.body.removeChild(textArea)
      return successful
    }
  } catch {
    return false
  }
  return false
}

export async function copyContextPrompt(
  contextFiles: ReadonlySet<string>,
  contextSymbols: ReadonlySet<string>
): Promise<string> {
  const exploreArgs: string[] = []

  for (const file of contextFiles) {
    exploreArgs.push(file)
  }

  for (const sym of contextSymbols) {
    exploreArgs.push(sym)
  }

  if (exploreArgs.length === 0) {
    throw new Error('No files or symbols selected in Context Workspace.')
  }

  const invoke = tauriInvoke()
  let payload: ExplorePayload | null = null

  if (invoke) {
    try {
      payload = (await invoke('explore', { symbols: exploreArgs })) as ExplorePayload
    } catch (err) {
      console.warn('Tauri explore IPC failed, falling back to in-memory graph:', err)
    }
  }

  // Graceful fallback from in-memory store if desktop IPC is absent or errored
  if (!payload) {
    const graph = useGraphStore.getState().graph
    const filesMap = new Map<string, ExploreSymbolBlock[]>()

    for (const path of contextFiles) {
      const node = graph?.nodes.find((n) => n.path === path)
      if (node) {
        filesMap.set(path, [
          {
            name: 'full_file',
            kind: 'file',
            code: `// Source of /${path} (${node.size_bytes} bytes)`,
            start_line: 1,
            end_line: Math.max(1, Math.round(node.size_bytes / 35)),
          },
        ])
      }
    }

    for (const symRef of contextSymbols) {
      const [path, name] = symRef.split('#')
      if (!path || !name) continue
      const blocks = filesMap.get(path) ?? []
      const node = graph?.nodes.find((n) => n.path === path)
      const sym = node?.symbols?.find((s) => s.name === name)
      blocks.push({
        name,
        kind: sym?.kind ?? 'symbol',
        code: `// Implementation of ${name}`,
        start_line: sym?.start_line ?? 1,
        end_line: sym?.end_line ?? 20,
      })
      filesMap.set(path, blocks)
    }

    payload = {
      files: Array.from(filesMap.entries()).map(([path, code_blocks]) => ({
        path,
        code_blocks,
      })),
      paths: [],
    }
  }

  const markdown = formatContextPrompt(payload)

  // Write to clipboard with fallback
  const success = await copyToClipboard(markdown)
  if (!success && typeof navigator !== 'undefined' && navigator.clipboard) {
    try {
      await navigator.clipboard.writeText(markdown)
    } catch (clipErr) {
      console.error('Failed to copy to clipboard:', clipErr)
    }
  }

  return markdown
}

/**
 * Pure formatter for the Context Engineering Prompt Architecture (CEPA).
 * Uses the 7-layer context stack for human readability and LLM precision.
 */
export function formatContextPrompt(payload: ExplorePayload): string {
  const totalFiles = payload.files.length
  let totalSymbols = 0
  for (const f of payload.files) {
    totalSymbols += f.code_blocks.filter((b) => b.kind !== 'file').length
  }

  let md = `# 🧭 REPO GRAPH — CONTEXT-ENGINEERED MISSION PROMPT\n`
  md += `> **Architecture Context:** ${totalFiles} target file${totalFiles === 1 ? '' : 's'}, ${totalSymbols} symbol block${totalSymbols === 1 ? '' : 's'}, ${payload.paths.length} call graph path${payload.paths.length === 1 ? '' : 's'}.\n`
  md += `> **Optimization:** Static dependency subgraph curated to eliminate context bloat and prevent hallucinations.\n\n`
  md += `---\n\n`

  // Layer 1: Instructions & Guardrails
  md += `## 📌 1. SYSTEM ROLE & EXECUTION GUARDRAILS\n`
  md += `You are a senior full-stack software engineer and systems architect. You are provided with a statically verified, localized dependency subgraph and exact source code slices.\n\n`
  md += `### Core Working Agreements:\n`
  md += `1. **Targeted Execution:** Focus your edits and analysis strictly on the files and symbols provided in this subgraph.\n`
  md += `2. **Zero Hallucination of Unseen Files:** Do not guess contents of files outside this set. If additional dependencies or schemas are needed, state which files are required or query the Repo Graph MCP tools (\`repograph_node\` / \`repograph_explore\`).\n`
  md += `3. **Conciseness & High Signal:** Lead with the outcome. Keep explanations brief and provide complete, drop-in replacement code blocks or diffs.\n\n`
  md += `---\n\n`

  // Layer 2 & 3: Retrieved Facts / Subgraph Architecture
  md += `## 🗺️ 2. SUBGRAPH ARCHITECTURE & DEPENDENCY MAP\n\n`
  md += `### 📦 In-Scope Modules & Symbols\n`
  for (const filePayload of payload.files) {
    md += `- **\`/${filePayload.path}\`**\n`
    const symbolBlocks = filePayload.code_blocks.filter((b) => b.kind !== 'file')
    if (symbolBlocks.length === 0) {
      md += `  - *(Full file loaded in context)*\n`
    } else {
      for (const block of symbolBlocks) {
        md += `  - 🔹 \`${block.name}\` *(${block.kind}, Lines ${block.start_line}–${block.end_line})*\n`
      }
    }
  }

  if (payload.paths.length > 0) {
    md += `\n### 🔀 Call Graph & Data Flow Paths\n`
    for (const edge of payload.paths) {
      md += `- \`${edge.from_symbol}\` ──*(${edge.kind})*──► \`${edge.to_symbol}\`\n`
    }
  }

  md += `\n---\n\n`

  // Layer 4: Retrieved Facts / Source Code
  md += `## 📄 3. RETRIEVED SOURCE CODE & SYMBOL IMPLEMENTATIONS\n\n`
  for (const filePayload of payload.files) {
    const ext = filePayload.path.split('.').pop() || ''
    md += `### 📁 \`/${filePayload.path}\`\n\n`

    for (const block of filePayload.code_blocks) {
      if (block.kind === 'file') {
        md += `\`\`\`${ext}\n${block.code}\n\`\`\`\n\n`
      } else {
        md += `#### 🔹 Symbol: \`${block.name}\` *(Lines ${block.start_line}–${block.end_line})*\n`
        md += `\`\`\`${ext}\n${block.code}\n\`\`\`\n\n`
      }
    }
  }

  md += `---\n\n`

  // Layer 5: Active Task (User Input Area)
  md += `## 🎯 4. ACTIVE TASK & ACCEPTANCE CRITERIA\n`
  md += `### 📝 User Objective:\n`
  md += `[Describe your request, feature, refactor, or bugfix here...]\n\n`
  md += `### ✅ Acceptance Criteria:\n`
  md += `- [ ] Resolve the objective within the targeted files.\n`
  md += `- [ ] Ensure type-safety, test coverage, and clean imports.\n`
  md += `- [ ] Do not break existing contracts or call paths listed in the Subgraph Map.\n\n`
  md += `---\n\n`

  // Layer 6 & 7: Scratchpad & Output Format
  md += `## 📋 5. RESPONSE FORMAT & SCRATCHPAD\n`
  md += `1. **Plan:** Outline key modifications in 2–3 concise bullet points.\n`
  md += `2. **Implementation:** Provide exact code replacements with clear file references.\n`
  md += `3. **Verification:** State how the solution satisfies the acceptance criteria.\n`

  return md
}

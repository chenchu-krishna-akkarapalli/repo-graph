import { describe, it, expect } from 'vitest'
import { formatContextPrompt, type ExplorePayload } from './promptExporter'

describe('formatContextPrompt', () => {
  it('formats clean human-readable and agent-optimal Markdown with the 7-layer context stack', () => {
    const mockPayload: ExplorePayload = {
      files: [
        {
          path: 'src/store.ts',
          code_blocks: [
            {
              name: 'useGraphStore',
              kind: 'function',
              code: 'export const useGraphStore = create(...)',
              start_line: 10,
              end_line: 50,
            },
          ],
        },
        {
          path: 'src/types.ts',
          code_blocks: [
            {
              name: 'full_file',
              kind: 'file',
              code: 'export interface GraphNode { ... }',
              start_line: 1,
              end_line: 100,
            },
          ],
        },
      ],
      paths: [
        {
          from_symbol: 'src/App.tsx#App',
          to_symbol: 'src/store.ts#useGraphStore',
          kind: 'calls',
        },
      ],
    }

    const md = formatContextPrompt(mockPayload)

    // Check header & metadata badges
    expect(md).toContain('# 🧭 REPO GRAPH — CONTEXT-ENGINEERED MISSION PROMPT')
    expect(md).toContain('2 target files')
    expect(md).toContain('1 symbol block')
    expect(md).toContain('1 call graph path')

    // Check Seven-Piece Context Stack Layers
    expect(md).toContain('## 📌 1. SYSTEM ROLE & EXECUTION GUARDRAILS')
    expect(md).toContain('## 🗺️ 2. SUBGRAPH ARCHITECTURE & DEPENDENCY MAP')
    expect(md).toContain('## 📄 3. RETRIEVED SOURCE CODE & SYMBOL IMPLEMENTATIONS')
    expect(md).toContain('## 🎯 4. ACTIVE TASK & ACCEPTANCE CRITERIA')
    expect(md).toContain('## 📋 5. RESPONSE FORMAT & SCRATCHPAD')

    // Check file and symbol details
    expect(md).toContain('`useGraphStore` *(function, Lines 10–50)*')
    expect(md).toContain('*(Full file loaded in context)*')
    expect(md).toContain('`src/App.tsx#App` ──*(calls)*──► `src/store.ts#useGraphStore`')
  })
})

import { tauriInvoke } from './loadGraph'

/**
 * Assembles and exports the Seven-Piece Context Stack in Markdown,
 * containing instructions, the localized manifest, and full file contents.
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
  let payload: ExplorePayload

  if (invoke) {
    try {
      payload = (await invoke('explore', { symbols: exploreArgs })) as ExplorePayload
    } catch (err) {
      throw new Error(`Failed to explore context: ${err}`, { cause: err })
    }
  } else {
    throw new Error('Context exploration is only supported in the desktop app.')
  }

  let markdown = `# SYSTEM INSTRUCTIONS
You are an expert AI software engineer. You are provided with a localized codebase subgraph and the full contents of the target files to edit. Focus your modifications strictly on these files and their dependencies. Do not guess file contents outside this set.

`;

  markdown += `## Localized Subgraph Manifest
This list contains the target files, their dependencies, and call graph paths.

### Files & Symbols in Context
`;

  for (const filePayload of payload.files) {
    markdown += `- /${filePayload.path}\n`
    for (const block of filePayload.code_blocks) {
      if (block.kind !== 'file') {
        markdown += `  - Symbol: ${block.name} (${block.kind}, Lines ${block.start_line}-${block.end_line})\n`
      }
    }
  }

  if (payload.paths.length > 0) {
    markdown += `\n### Call Graph Paths\n`
    for (const edge of payload.paths) {
      markdown += `- ${edge.from_symbol} → ${edge.to_symbol} (${edge.kind})\n`
    }
  }

  markdown += `\n`;

  markdown += `## File Contents
Below is the source code of the selected files or specific symbols.

`;

  for (const filePayload of payload.files) {
    const ext = filePayload.path.split('.').pop() || ''
    markdown += `### /${filePayload.path}\n`
    
    for (const block of filePayload.code_blocks) {
      if (block.kind === 'file') {
        markdown += `\`\`\`${ext}\n${block.code}\n\`\`\`\n\n`
      } else {
        markdown += `#### Symbol: ${block.name} (Lines ${block.start_line}-${block.end_line})\n\`\`\`${ext}\n${block.code}\n\`\`\`\n\n`
      }
    }
  }

  markdown += `## Active Task
[Describe your request or task here...]
`

  await navigator.clipboard.writeText(markdown)

  return markdown
}

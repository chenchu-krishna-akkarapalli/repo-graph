import type { FileTreeNode } from '../types'

/**
 * Recursively builds an ASCII representation of the FileTreeNode tree.
 * Connector conventions follow standard tree formatting:
 * ├── for sibling items
 * └── for the last sibling item in a folder
 * │   for open parents with further children below
 */
export function generateAsciiLayout(
  activeProjectRoot: string | null,
  fileTree: FileTreeNode[]
): string {
  let rootName = 'root'
  if (activeProjectRoot) {
    const normalized = activeProjectRoot.replace(/\\/g, '/')
    const parts = normalized.split('/')
    const last = parts[parts.length - 1]
    if (last) {
      rootName = last
    }
  }

  let output = `## Code Layout\n\`\`\`\n${rootName}/\n`

  const sortNodes = (nodes: FileTreeNode[]): FileTreeNode[] => {
    return [...nodes].sort((a, b) => {
      if (a.is_dir && !b.is_dir) return -1
      if (!a.is_dir && b.is_dir) return 1
      return a.name.localeCompare(b.name)
    })
  }

  const recurse = (nodes: FileTreeNode[], prefix: string) => {
    const sorted = sortNodes(nodes)
    for (let i = 0; i < sorted.length; i++) {
      const node = sorted[i]
      const isLast = i === sorted.length - 1
      const connector = isLast ? '└── ' : '├── '
      const suffix = node.is_dir ? '/' : ''
      
      output += `${prefix}${connector}${node.name}${suffix}\n`

      if (node.is_dir && node.children && node.children.length > 0) {
        const nextPrefix = prefix + (isLast ? '    ' : '│   ')
        recurse(node.children, nextPrefix)
      }
    }
  }

  recurse(fileTree, '')
  output += '```\n'
  return output
}
export async function copyAsciiLayout(
  activeProjectRoot: string | null,
  fileTree: FileTreeNode[]
): Promise<string> {
  const layout = generateAsciiLayout(activeProjectRoot, fileTree)
  await navigator.clipboard.writeText(layout)
  return layout
}

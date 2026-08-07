import type { FileTreeNode } from '../types'

/**
 * Browser fallback for the explorer: derive a directory tree from the
 * indexed graph's file paths when the native `read_directory_tree` IPC
 * command isn't available. Folders first, then files, alphabetical.
 */
export function buildTreeFromPaths(paths: string[]): FileTreeNode[] {
  interface Dir {
    dirs: Map<string, Dir>
    files: string[] // full paths
  }
  const rootDir: Dir = { dirs: new Map(), files: [] }

  for (const path of paths) {
    const segments = path.split('/')
    let current = rootDir
    for (const segment of segments.slice(0, -1)) {
      let next = current.dirs.get(segment)
      if (!next) {
        next = { dirs: new Map(), files: [] }
        current.dirs.set(segment, next)
      }
      current = next
    }
    current.files.push(path)
  }

  function emit(dir: Dir, prefix: string): FileTreeNode[] {
    const folders = [...dir.dirs.entries()]
      .sort(([a], [b]) => a.toLowerCase().localeCompare(b.toLowerCase()))
      .map(([name, child]) => {
        const path = prefix === '' ? name : `${prefix}/${name}`
        return { name, path, is_dir: true, children: emit(child, path) }
      })
    const files = dir.files
      .sort((a, b) => a.toLowerCase().localeCompare(b.toLowerCase()))
      .map((path) => ({
        name: path.split('/').pop() ?? path,
        path,
        is_dir: false,
        children: [],
      }))
    return [...folders, ...files]
  }
  return emit(rootDir, '')
}

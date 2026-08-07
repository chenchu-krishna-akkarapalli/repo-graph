import { memo } from 'react'
import { Handle, Position, type NodeProps } from 'reactflow'
import { Folder } from 'lucide-react'
import { useGraphStore } from '../../store'
import type { FolderNodeData } from '../../lib/layout'

/** Collapsed directory container — click to expand back into its files. */
function CustomFolderNode({ id, data }: NodeProps<FolderNodeData>) {
  const toggleDir = useGraphStore((s) => s.toggleDir)
  return (
    <div
      data-node-path={id}
      data-node-neighbors="|"
      className="graph-node flex h-full cursor-pointer items-center gap-2 rounded-xl border border-dashed border-slate-600/60 bg-[#11151E]/90 px-3 text-white/80 hover:border-violet-400/80 hover:bg-[#1a202c]/90 backdrop-blur-md shadow-md"
      onClick={() => toggleDir(data.dir)}
      title={`Expand /${data.dir}`}
    >
      <Handle type="target" position={Position.Right} className="!h-2.5 !w-2.5 !border-0 !bg-white/30" />
      <Folder size={15} className="shrink-0 text-slate-400" />
      <div className="min-w-0">
        <div className="truncate text-[13px] font-medium text-white/90">/{data.dir}</div>
        <div className="font-mono text-[10px] text-white/40">{data.fileCount} files collapsed</div>
      </div>
      <Handle type="source" position={Position.Left} className="!h-2.5 !w-2.5 !border-0 !bg-white/30" />
    </div>
  )
}

export default memo(CustomFolderNode)

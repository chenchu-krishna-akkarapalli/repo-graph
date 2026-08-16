import { useCallback, useEffect, useState } from 'react'
import {
  Check,
  Copy,
  Search,
  Compass,
  X,
  Zap,
  Sparkles,
  Layers,
  ShieldCheck,
  Cpu,
  FileCode2,
} from 'lucide-react'
import { isCepaDismissed, setCepaDismissed } from '../lib/cepaPreferences'

type GuideTab = 'quick' | 'master' | 'stack'

interface PromptTabOption {
  id: GuideTab
  title: string
  subtitle: string
  badge: string
  blurb: string
  body: string
}

const PROMPT_OPTIONS: PromptTabOption[] = [
  {
    id: 'quick',
    title: 'Quick 1-Liner Agent Directive',
    subtitle: 'Quick Rule',
    badge: 'Fast Paste',
    blurb: 'Drop this into Claude Desktop, Cursor, or ChatGPT system instructions to enforce Repo Graph MCP exploration.',
    body: `You are equipped with the Repo Graph MCP Server (server: "repo-graph").

CRITICAL DIRECTIVE: Never grep or crawl full source files speculatively.
Follow this 3-step sequence:
1. MAP: Call \`repograph_files(top_k: 5)\` or \`repograph_search(query)\` to locate the target architecture.
2. INSPECT: Call \`repograph_explore(symbols, signature_only: true)\` to view declarations and call paths.
3. EDIT: Call \`repograph_explore(symbols)\` to load full implementations only when ready to apply modifications.

Keep responses concise and lead with the concrete outcome.`,
  },
  {
    id: 'master',
    title: 'Production Master System Prompt',
    subtitle: 'Master Prompt',
    badge: 'Production',
    blurb: 'The complete context-engineered system prompt incorporating Claude Opus 5 directives and persistent MCP session management.',
    body: `# REPO GRAPH — MASTER SYSTEM PROMPT

You are Repo Graph AI, a principal full-stack software engineer operating with an offline-first codebase dependency graph engine.

## 1. OPERATIONAL DIRECTIVES
- Outcome-First Delivery: Lead every answer with the concrete outcome ("what happened" / "what was found"), followed by essential details.
- Narration Cadence: Before your first tool call, state in one sentence what you are about to do. While executing, update only when uncovering critical findings.
- Conciseness: Keep responses focused and free of unprompted boilerplate.

## 2. WORKING AGREEMENTS & GUARDRAILS
1. Targeted Subgraph Scope: Operate strictly on the files and symbols in the active context. Never hallucinate unseen files.
2. Centrality Ranking Over Full Ingestion: Prefer \`repograph_files(top_k: 5)\` and \`repograph_explore\` over whole-repo reads (99%+ token savings).
3. Static Analysis Only: Never execute arbitrary runtime code or project build scripts during static analysis turns.

## 3. RESIDENT MCP TOOLKIT
- \`repograph_status\`: Check active root, persistent session health, and background sync state.
- \`repograph_files\`: Centrality-ranked architecture map (filter with top_k, min_rank, or scope).
- \`repograph_node\`: Fetch exact file contents or extracted symbol line slices.
- \`repograph_explore\`: Compound extraction of symbol declarations, bodies, and call paths in a single call.
- \`repograph_callers\` & \`repograph_callees\`: Inbound and outbound call-graph navigation.
- \`repograph_impact\`: Transitive blast-radius analysis before refactoring.
- \`repograph_search\`: Fast SQLite FTS symbol search across tokenized names.

<tone_preference>
Keep outputs reasonably concise.
</tone_preference>`,
  },
  {
    id: 'stack',
    title: 'The Seven-Piece Context Stack',
    subtitle: '7-Layer Architecture',
    badge: 'Theory & UX',
    blurb: 'How Repo Graph organizes runtime information to eliminate token waste and prevent hallucinations.',
    body: `/* THE SEVEN-PIECE CONTEXT STACK */

1. [INSTRUCTIONS]     Role, Execution Guardrails, and Working Agreements.
2. [USER INPUT]       Active Task, User Objective, and Acceptance Criteria.
3. [RETRIEVED FACTS]  Ranked Subgraph Map, Call Graph Paths, and Symbol Slices.
4. [TOOLS]            Persistent MCP Server Tools (explore, search, files, impact).
5. [SHORT-TERM NOTES] Scratchpad, Step Status, and Execution Plan.
6. [LONG-TERM MEMORY] Architecture Constraints and Immutable Project Rules.
7. [OUTPUT FORMAT]    Structured Diffs, Type-Safe Code, and Outcome-First Speech.`,
  },
]

const WORKFLOW_STEPS = [
  {
    icon: Compass,
    badge: 'Step 1',
    title: 'Map the Subgraph',
    command: 'repograph_files(top_k: 5)',
    description: 'Centrality ranking highlights the core 5–10 structural files, cutting 90% of file noise.',
    accent: 'border-cyan-500/30 bg-cyan-500/10 text-cyan-400',
  },
  {
    icon: Search,
    badge: 'Step 2',
    title: 'Target Symbols',
    command: 'repograph_search("query")',
    description: 'Instant SQLite FTS index resolves CamelCase and snake_case symbols with line ranges.',
    accent: 'border-blue-500/30 bg-blue-500/10 text-blue-400',
  },
  {
    icon: Cpu,
    badge: 'Step 3',
    title: 'Compound Explore',
    command: 'repograph_explore(symbols)',
    description: 'Extracts exact code slices and bidirectional call graphs in 1 single JSON payload.',
    accent: 'border-emerald-500/30 bg-emerald-500/10 text-emerald-400',
  },
]

const STACK_LAYERS = [
  { num: '1', name: 'Instructions', desc: 'System role & zero-hallucination guardrails', icon: ShieldCheck },
  { num: '2', name: 'User Task', desc: 'Active prompt with explicit acceptance criteria', icon: FileCode2 },
  { num: '3', name: 'Retrieved Facts', desc: 'Ranked subgraph map & exact line-sliced symbols', icon: Layers },
  { num: '4', name: 'Tools (MCP)', desc: '8 resident MCP tools with persistent session tracking', icon: Cpu },
  { num: '5', name: 'Scratchpad', desc: 'Execution checklist & intermediate plan notes', icon: Sparkles },
  { num: '6', name: 'Memory', desc: 'Offline-first invariants & project constraints', icon: Compass },
  { num: '7', name: 'Output Format', desc: 'Complete drop-in code replacements & clean diffs', icon: Zap },
]

/**
 * Robust clipboard copy helper with fallback for webview focus constraints.
 */
async function copyToClipboard(text: string): Promise<boolean> {
  try {
    if (navigator.clipboard && window.isSecureContext) {
      await navigator.clipboard.writeText(text)
      return true
    }
  } catch {
    // Fallback below
  }

  try {
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
  } catch {
    return false
  }
}

export default function CEPAUserGuideModal({
  open,
  onClose,
}: {
  open: boolean
  onClose: () => void
}) {
  const [activeTab, setActiveTab] = useState<GuideTab>('quick')
  const [copied, setCopied] = useState(false)
  const [copyError, setCopyError] = useState<string | null>(null)
  const [dontShow, setDontShow] = useState(false)

  useEffect(() => {
    if (open) {
      // eslint-disable-next-line react-hooks/set-state-in-effect
      setDontShow(isCepaDismissed())
      setCopied(false)
      setCopyError(null)
    }
  }, [open])

  const handleClose = useCallback(() => {
    try {
      setCepaDismissed(dontShow)
    } catch {
      // Best-effort storage sync
    }
    onClose()
  }, [dontShow, onClose])

  useEffect(() => {
    if (!open) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') handleClose()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [open, handleClose])

  const handleCopy = async () => {
    const activeOption = PROMPT_OPTIONS.find((p) => p.id === activeTab)
    if (!activeOption) return

    const success = await copyToClipboard(activeOption.body)
    if (success) {
      setCopied(true)
      setCopyError(null)
      setTimeout(() => setCopied(false), 2000)
    } else {
      setCopyError('Unable to write to clipboard. Please copy manually.')
    }
  }

  if (!open) return null

  const activeOption = PROMPT_OPTIONS.find((p) => p.id === activeTab)!

  return (
    <div
      className="fixed inset-0 z-[60] flex items-center justify-center bg-black/80 p-4 backdrop-blur-md animate-fade-in"
      onClick={handleClose}
      role="presentation"
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="cepa-title"
        onClick={(e) => e.stopPropagation()}
        className="max-h-[88vh] h-[700px] w-full max-w-3xl overflow-hidden rounded-2xl border border-white/15 bg-[#0B0E14]/95 shadow-2xl backdrop-blur-2xl flex flex-col text-white"
      >
        {/* Pinned Header Section */}
        <div className="shrink-0 p-6 pb-4 border-b border-white/10 flex items-start justify-between gap-4 bg-[#0E121A]/50">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <span className="flex h-5 items-center gap-1.5 rounded-md border border-blue-500/30 bg-blue-500/10 px-2 py-0.5 text-[10px] font-bold uppercase tracking-wider text-blue-400">
                <Sparkles size={11} /> Context Engineering Guide
              </span>
              <span className="text-[10px] font-semibold text-emerald-400 bg-emerald-500/10 px-2 py-0.5 rounded-md border border-emerald-500/20">
                99.15% Token Savings
              </span>
            </div>
            <h2
              id="cepa-title"
              className="mt-1.5 text-lg font-bold tracking-tight text-white/95"
            >
              How Context Engineering Supercharges AI Agents
            </h2>
            <p className="mt-0.5 text-xs text-white/50 leading-relaxed max-w-xl">
              AI agents don't need entire codebases dumped into context. Repo Graph serves a centrality-ranked dependency map and exact symbol slices on demand.
            </p>
          </div>
          <button
            onClick={handleClose}
            aria-label="Close CEPA guide"
            className="shrink-0 cursor-pointer rounded-lg border border-white/10 bg-white/[0.04] p-1.5 text-white/50 transition-all hover:bg-white/10 hover:text-white"
          >
            <X size={16} />
          </button>
        </div>

        {/* Scrollable Modal Body */}
        <div className="flex-1 overflow-y-auto p-6 space-y-5 scrollbar-thin">
          {/* Visual Comparison: The Old Way vs The Repo Graph Way */}
          <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
            <div className="rounded-xl border border-rose-500/20 bg-rose-950/10 p-3.5 flex flex-col justify-between gap-2">
              <div>
                <div className="flex items-center justify-between text-[11px] font-bold uppercase tracking-wider text-rose-400">
                  <span>❌ Naive Full-Repo Ingestion</span>
                  <span className="font-mono">~32,000 Tokens</span>
                </div>
                <p className="mt-1.5 text-[11px] text-white/50 leading-normal">
                  Dumps 100+ raw source files into context. Agent slows down, loses reasoning focus, and costs $0.10+ per prompt turn.
                </p>
              </div>
              <div className="h-1.5 w-full rounded-full bg-rose-500/20 overflow-hidden">
                <div className="h-full bg-rose-500 rounded-full w-full" />
              </div>
            </div>

            <div className="rounded-xl border border-emerald-500/30 bg-emerald-950/15 p-3.5 flex flex-col justify-between gap-2">
              <div>
                <div className="flex items-center justify-between text-[11px] font-bold uppercase tracking-wider text-emerald-400">
                  <span>✅ Repo Graph Subgraph Map</span>
                  <span className="font-mono text-emerald-300">~275 Tokens</span>
                </div>
                <p className="mt-1.5 text-[11px] text-white/50 leading-normal">
                  Serves centrality-ranked architecture map + exact line-sliced symbols in 1 call. Near-instant response and $0.0008 cost.
                </p>
              </div>
              <div className="h-1.5 w-full rounded-full bg-white/[0.05] overflow-hidden">
                <div className="h-full bg-gradient-to-r from-emerald-500 to-cyan-400 rounded-full w-[1.5%]" />
              </div>
            </div>
          </div>

          {/* 3-Step Execution Workflow Cards */}
          <div className="space-y-2">
            <h3 className="text-[11px] font-bold uppercase tracking-wider text-white/40 flex items-center gap-1.5">
              <Zap size={13} className="text-amber-400" />
              3-Step Targeted Exploration Workflow
            </h3>
            <div className="grid grid-cols-1 md:grid-cols-3 gap-2.5">
              {WORKFLOW_STEPS.map((step) => {
                const Icon = step.icon
                return (
                  <div
                    key={step.badge}
                    className="rounded-xl border border-white/10 bg-white/[0.02] p-3 flex flex-col justify-between gap-2 transition-all hover:bg-white/[0.04]"
                  >
                    <div className="flex items-center justify-between">
                      <span className={`flex h-7 w-7 items-center justify-center rounded-lg border ${step.accent}`}>
                        <Icon size={14} />
                      </span>
                      <span className="font-mono text-[10px] font-bold uppercase text-white/30">{step.badge}</span>
                    </div>
                    <div>
                      <h4 className="text-xs font-semibold text-white/90">{step.title}</h4>
                      <p className="mt-0.5 text-[11px] leading-relaxed text-white/45">{step.description}</p>
                    </div>
                    <code className="rounded bg-black/50 px-2 py-1 font-mono text-[10px] text-white/70 border border-white/5 truncate">
                      {step.command}
                    </code>
                  </div>
                )
              })}
            </div>
          </div>

          {/* The Seven-Piece Context Stack Visual Layer */}
          {activeTab === 'stack' && (
            <div className="rounded-xl border border-blue-500/20 bg-blue-950/10 p-4 space-y-3">
              <div className="flex items-center justify-between">
                <h4 className="text-xs font-bold uppercase tracking-wider text-blue-300 flex items-center gap-1.5">
                  <Layers size={14} /> The Seven-Piece Context Stack Breakdown
                </h4>
                <span className="text-[10px] text-white/40 font-mono">Curated at Runtime</span>
              </div>
              <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
                {STACK_LAYERS.map((l) => {
                  const Icon = l.icon
                  return (
                    <div key={l.num} className="flex items-start gap-2.5 rounded-lg border border-white/5 bg-black/30 p-2.5">
                      <span className="flex h-5 w-5 shrink-0 items-center justify-center rounded bg-blue-500/20 text-[10px] font-mono font-bold text-blue-300">
                        {l.num}
                      </span>
                      <div className="min-w-0">
                        <span className="text-[11px] font-semibold text-white/85 flex items-center gap-1">
                          <Icon size={12} className="text-blue-400" /> {l.name}
                        </span>
                        <p className="text-[10px] text-white/40 leading-tight mt-0.5">{l.desc}</p>
                      </div>
                    </div>
                  )
                })}
              </div>
            </div>
          )}

          {/* Prompt Exporter Tabs & Copier */}
          <div className="space-y-3">
            <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2">
              <h3 className="text-[11px] font-bold uppercase tracking-wider text-white/40">
                Select Agent Prompt Architecture
              </h3>
              {/* Segmented control tabs */}
              <div className="flex rounded-lg bg-black/50 p-1 border border-white/10 gap-1 shrink-0">
                {PROMPT_OPTIONS.map((p) => (
                  <button
                    key={p.id}
                    onClick={() => {
                      setActiveTab(p.id)
                      setCopied(false)
                      setCopyError(null)
                    }}
                    className={[
                      'h-7 px-3 rounded-md text-[10px] font-semibold tracking-wider transition-all cursor-pointer border-0 flex items-center gap-1.5',
                      activeTab === p.id
                        ? 'bg-blue-600 text-white shadow-sm font-bold'
                        : 'text-white/40 hover:text-white/80 hover:bg-white/[0.04]',
                    ].join(' ')}
                  >
                    <span>{p.subtitle}</span>
                  </button>
                ))}
              </div>
            </div>

            {/* Tab Content Box */}
            <div className="rounded-xl border border-white/10 bg-[#07090D] shadow-inner flex flex-col overflow-hidden">
              <div className="p-3.5 border-b border-white/10 flex items-start justify-between gap-4 bg-white/[0.02]">
                <div className="min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="text-xs font-bold text-white/90">{activeOption.title}</span>
                    <span className="rounded bg-white/10 px-1.5 py-0.5 text-[9px] font-mono text-white/60">
                      {activeOption.badge}
                    </span>
                  </div>
                  <span className="text-[11px] text-white/40 block mt-1 leading-normal">{activeOption.blurb}</span>
                </div>
                <button
                  onClick={() => void handleCopy()}
                  className={[
                    'flex h-8 shrink-0 items-center justify-center gap-1.5 rounded-lg border px-3.5 text-xs font-semibold tracking-wider transition-all cursor-pointer border-0',
                    copied
                      ? 'bg-emerald-500/20 text-emerald-300 border border-emerald-500/30 shadow-[0_0_12px_rgba(16,185,129,0.2)]'
                    : 'bg-white/10 hover:bg-white/20 text-white border border-white/15 active:scale-[0.98]',
                  ].join(' ')}
                >
                  {copied ? <Check size={13} className="text-emerald-400 animate-pulse" /> : <Copy size={13} />}
                  {copied ? 'Copied!' : 'Copy Prompt'}
                </button>
              </div>

              <pre className="p-4 font-mono text-[11px] leading-relaxed text-white/70 bg-black/40 select-all overflow-x-auto">
                {activeOption.body}
              </pre>
            </div>

            {copyError && (
              <div className="rounded-lg border border-rose-500/30 bg-rose-500/10 px-3 py-2 text-[11px] text-rose-300">
                {copyError}
              </div>
            )}
          </div>
        </div>

        {/* Pinned Footer & Dismissal */}
        <div className="shrink-0 px-6 py-4 border-t border-white/10 bg-[#080B10]/80 flex items-center justify-between gap-4">
          <label className="flex cursor-pointer items-center gap-2 text-[11px] text-white/50 hover:text-white/70 select-none">
            <input
              type="checkbox"
              checked={dontShow}
              onChange={(e) => setDontShow(e.target.checked)}
              className="h-3.5 w-3.5 cursor-pointer accent-blue-500 bg-black/40 border border-white/10 rounded"
            />
            Don’t show this guide on startup
          </label>
          <button
            onClick={handleClose}
            className="h-8.5 shrink-0 cursor-pointer rounded-lg bg-blue-600 hover:bg-blue-500 text-xs font-semibold text-white shadow-lg shadow-blue-950/40 border border-blue-400/20 active:scale-[0.98] px-4 transition-all"
          >
            Got it, Let's Code!
          </button>
        </div>
      </div>
    </div>
  )
}

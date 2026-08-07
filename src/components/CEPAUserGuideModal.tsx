import { useCallback, useEffect, useState } from 'react'
import { Check, Copy, Search, Compass, Scissors, X, Zap, Sparkles, BookOpen } from 'lucide-react'
import { isCepaDismissed, setCepaDismissed } from '../lib/cepaPreferences'

/**
 * Interactive Context Engineering Prompt Architecture (CEPA) guide modal.
 *
 * Designed with a premium glassmorphic visual layout adhering to docs/UI_UX_DESIGN_SYSTEM.md
 * and incorporating the 1-to-2 call explore strategies.
 */

interface PromptCard {
  id: string
  title: string
  subtitle: string
  blurb: string
  body: string
}

const PROMPTS: PromptCard[] = [
  {
    id: 'minimal',
    title: 'Minimal Quick Rule',
    subtitle: '1-Liner Guardrail',
    blurb: 'Drop this into any agent\'s system prompt to stop blind grepping and file crawling.',
    body: `This workspace is indexed by Repo Graph (MCP server \`repo-graph\`).

Do not grep, glob, or read files at random. Follow this sequence:
1. ORIENT  — Call \`repograph_status\` and \`repograph_files(scope)\` to map the area.
2. TARGET  — Call \`repograph_search(query)\` to find candidate symbol names.
3. EXPLORE — Call \`repograph_explore(symbols, signature_only: true)\` to inspect declarations and caller graphs.
4. WRITE   — Re-call \`repograph_explore(symbols)\` without signature_only to load bodies ONLY when ready to edit.

Never write code against a signature you have not expanded.`,
  },
  {
    id: 'full',
    title: 'Full Master System Prompt',
    subtitle: 'Production Prompt',
    blurb: 'The complete context engineering ruleset including the 8-tool MCP wire contract.',
    body: `# AGENT INSTRUCTION: REPO-GRAPH HIGH-EFFICIENCY CODEBASE EXPLORATION

You are equipped with the Repo Graph MCP Server (\`repo-graph\`). Your objective is to perform codebase navigation and refactoring in 1 TO 2 TOOL CALLS MAX using the compound tool \`repograph_explore\`.

## CORE WIRE TOOL CONTRACT (8 Tools Available)
- \`repograph_explore(symbols: string[])\` -> ⚡ PRIMARY COMPOUND TOOL. Call this first! Accepts bare names ('buildFlow') or 'path#symbol' references. Returns JSON containing \`files\` (code blocks) and \`paths\` (call graph edges).
- \`repograph_search(query: string)\` -> Fuzzy search for symbol names across the repository.
- \`repograph_files(scope?: string)\` -> View compressed directory topology map.
- \`repograph_node(path: string, symbol?: string)\` -> Fetch specific file or symbol content.
- \`repograph_impact(path?: string, symbol?: string)\` -> Run blast-radius analysis before refactoring.
- \`repograph_callers(path?: string, symbol?: string)\` -> Fetch upstream caller graph.
- \`repograph_callees(path?: string, symbol?: string)\` -> Fetch downstream callee graph.
- \`repograph_status()\` -> Check index health and freshness.

## 1-TO-2 CALL EXECUTION STRATEGY
1. TURN 1 (PRIMARY): Call \`repograph_explore(symbols: ["TargetSymbol"])\` or \`repograph_search(query: "target")\`.
   - Do NOT call \`get_manifest\` or \`read_file\` (these are internal Rust names, not wire tool names).
   - \`repograph_explore\` returns code blocks and bi-directional call-graph edges (\`paths\`) in a single JSON payload.
2. TURN 2 (EDIT/RESPONSE): Apply targeted code modifications or present your answer to the user.
3. FOR HIGH-RISK INTERFACE CHANGES: Call \`repograph_impact(symbol: "TargetSymbol")\` before editing to inspect downstream impact.

Never browse raw source files speculatively. Rely on \`repograph_explore\` for 90%+ token efficiency.`,
  },
]

const STEPS = [
  {
    icon: Compass,
    badge: '01',
    label: 'Orient',
    colorClass: 'text-cyan-400 border-cyan-500/20 bg-cyan-950/20 shadow-[0_0_12px_rgba(6,182,212,0.15)]',
    calls: ['repograph_status', 'repograph_files(scope)'],
    detail: 'Confirm index health, then retrieve a compressed map of files, routes, and exports. Do not load raw code yet.',
  },
  {
    icon: Search,
    badge: '02',
    label: 'Target',
    colorClass: 'text-violet-400 border-violet-500/20 bg-violet-950/20 shadow-[0_0_12px_rgba(139,92,246,0.15)]',
    calls: ['repograph_search("store")'],
    detail: 'Search camelCase/snake_case symbol prefixes. Pinpoint exact file locations of variables and utilities in milliseconds.',
  },
  {
    icon: Scissors,
    badge: '03',
    label: 'Explore Leanly',
    colorClass: 'text-emerald-400 border-emerald-500/20 bg-emerald-950/20 shadow-[0_0_12px_rgba(16,185,129,0.15)]',
    calls: ['repograph_explore(symbols, signature_only: true)'],
    detail: 'Load declarations-first (skips function bodies to save 98.5% tokens). Expand bodies only when ready to apply edits.',
  },
]

/**
 * Robust clipboard copy helper with backup textarea fallback for non-secure
 * contexts or Tauri webview focus constraints.
 */
async function copyToClipboard(text: string): Promise<boolean> {
  try {
    if (navigator.clipboard && window.isSecureContext) {
      await navigator.clipboard.writeText(text)
      return true
    }
  } catch {
    // Clipboard API unavailable or permission denied; fall through to the
    // textarea path below.
  }

  // Fallback copy mechanism
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
  const [activeTab, setActiveTab] = useState<'minimal' | 'full'>('minimal')
  const [copied, setCopied] = useState(false)
  const [copyError, setCopyError] = useState<string | null>(null)
  const [dontShow, setDontShow] = useState(false)

  useEffect(() => {
    if (open) {
      // Same localStorage sync as in App.tsx, re-read each time the modal
      // opens so a dismissal elsewhere is reflected.
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
      // Best-effort storage write.
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
    const activePrompt = PROMPTS.find((p) => p.id === activeTab)
    if (!activePrompt) return

    const success = await copyToClipboard(activePrompt.body)
    if (success) {
      setCopied(true)
      setCopyError(null)
      setTimeout(() => setCopied(false), 2000)
    } else {
      setCopyError('Clipboard write block. Please copy the text below manually.')
    }
  }

  if (!open) return null

  const activePrompt = PROMPTS.find((p) => p.id === activeTab)!

  return (
    <div
      className="fixed inset-0 z-[60] flex items-center justify-center bg-black/80 p-4 backdrop-blur-md"
      onClick={handleClose}
      role="presentation"
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="cepa-title"
        onClick={(e) => e.stopPropagation()}
        className="max-h-[90vh] w-full max-w-2xl overflow-y-auto rounded-2xl border border-white/10 bg-[#0F1218]/95 p-6 shadow-2xl backdrop-blur-xl flex flex-col gap-5 text-white"
      >
        {/* Header Section */}
        <div className="flex items-start justify-between gap-4">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <Sparkles size={16} className="text-violet-400" />
              <span className="text-[10px] font-bold uppercase tracking-wider text-violet-400">Context Engineering Prompt Architecture</span>
            </div>
            <h2
              id="cepa-title"
              className="mt-1 bg-gradient-to-r from-violet-400 via-purple-300 to-cyan-400 bg-clip-text text-xl font-extrabold tracking-tight text-transparent"
            >
              Cut Your Agent's Context Ingestion by 98.56%
            </h2>
            <p className="mt-1 text-xs text-white/50 leading-relaxed">
              Transition from blind file crawling to targeted symbol exploration. Force your coding agent to work within a highly optimized 1-to-2 call budget.
            </p>
          </div>
          <button
            onClick={handleClose}
            aria-label="Close CEPA guide"
            className="shrink-0 cursor-pointer rounded-lg border border-white/10 bg-white/[0.03] p-1.5 text-white/40 transition-all hover:bg-white/10 hover:text-white/90"
          >
            <X size={16} />
          </button>
        </div>

        {/* Visual 3-Step Execution Workflow */}
        <div className="space-y-2">
          <h3 className="text-xs font-bold uppercase tracking-wider text-white/40 flex items-center gap-1.5">
            <BookOpen size={13} />
            Visual 3-Step Execution Workflow
          </h3>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
            {STEPS.map((step) => {
              const Icon = step.icon
              return (
                <div
                  key={step.label}
                  className="rounded-xl border border-white/10 bg-white/[0.02] p-4 flex flex-col justify-between gap-2.5 transition-all hover:bg-white/[0.04]"
                >
                  <div className="flex items-start justify-between">
                    <span className={`flex h-8 w-8 items-center justify-center rounded-lg border ${step.colorClass}`}>
                      <Icon size={16} />
                    </span>
                    <span className="font-mono text-xs font-bold text-white/20">{step.badge}</span>
                  </div>
                  <div>
                    <h4 className="text-sm font-semibold text-white/90">{step.label}</h4>
                    <p className="mt-1 text-[11px] leading-normal text-white/40">{step.detail}</p>
                  </div>
                  <div className="flex flex-wrap gap-1 mt-1">
                    {step.calls.map((c) => (
                      <code
                        key={c}
                        className="rounded bg-black/40 px-1.5 py-0.5 font-mono text-[9px] text-white/60 border border-white/5 truncate max-w-full"
                        title={c}
                      >
                        {c}
                      </code>
                    ))}
                  </div>
                </div>
              )
            })}
          </div>
        </div>

        {/* Interactive Savings Payoff Card */}
        <div className="rounded-xl border border-emerald-500/20 bg-emerald-950/10 p-4 space-y-2">
          <div className="flex items-center justify-between">
            <div>
              <h4 className="text-xs font-bold uppercase tracking-wider text-emerald-400 flex items-center gap-1.5">
                <Zap size={13} />
                Interactive Savings Payoff Card
              </h4>
              <p className="text-[11px] text-white/50 mt-0.5">
                Declarations-First Mode (<code className="text-emerald-300 text-[10px]">signature_only: true</code>) skips function bodies to save context tokens.
              </p>
            </div>
            <div className="text-right">
              <span className="text-lg font-extrabold text-emerald-400">98.56%</span>
              <span className="block text-[9px] uppercase tracking-wider text-white/30">Tokens Saved</span>
            </div>
          </div>

          {/* Visual Comparison Bar */}
          <div className="space-y-1.5">
            <div className="flex justify-between text-[10px] font-mono text-white/40">
              <span>Naive File Crawl: 27,620 tokens ($0.0828)</span>
              <span>Repo Graph Floor: 398 tokens ($0.0011)</span>
            </div>
            <div className="relative h-3 w-full bg-white/[0.04] rounded-full overflow-hidden border border-white/5">
              {/* Naive full bar (hidden/transparent background is the naive) */}
              <div className="absolute inset-y-0 left-0 bg-gradient-to-r from-emerald-500 to-teal-400 rounded-full transition-all duration-500" style={{ width: '1.44%' }} />
            </div>
            <div className="flex justify-between items-center text-[10px] text-white/30">
              <span>Ingested full file bodies</span>
              <span className="text-emerald-400 font-semibold">1,473 characters returned</span>
            </div>
          </div>
        </div>

        {/* One-Click Agent Rules Exporter */}
        <div className="space-y-3">
          <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 border-b border-white/10 pb-2">
            <h3 className="text-xs font-bold uppercase tracking-wider text-white/40">
              One-Click Agent Rules Exporter
            </h3>
            {/* Segmented control tabs */}
            <div className="flex rounded-lg bg-black/40 p-1 border border-white/5 gap-1 shrink-0">
              {PROMPTS.map((p) => (
                <button
                  key={p.id}
                  onClick={() => {
                    setActiveTab(p.id as 'minimal' | 'full')
                    setCopied(false)
                    setCopyError(null)
                  }}
                  className={[
                    'h-7 px-3 rounded-md text-[10px] font-bold uppercase tracking-wider transition-all cursor-pointer border-0',
                    activeTab === p.id
                      ? 'bg-white/10 text-white shadow-sm border border-white/5'
                      : 'text-white/40 hover:text-white/80 hover:bg-white/[0.02]',
                  ].join(' ')}
                >
                  {p.subtitle}
                </button>
              ))}
            </div>
          </div>

          {/* Tab Content Panels */}
          <div className="rounded-xl border border-white/10 bg-[#090A0F]/60 shadow-inner flex flex-col overflow-hidden">
            <div className="p-3 border-b border-white/5 flex items-start justify-between gap-4 bg-white/[0.01]">
              <div className="min-w-0">
                <span className="text-xs font-semibold text-white/85 block">{activePrompt.title}</span>
                <span className="text-[11px] text-white/40 block mt-0.5">{activePrompt.blurb}</span>
              </div>
              <button
                onClick={() => void handleCopy()}
                className={[
                  'flex h-9 shrink-0 items-center justify-center gap-2 rounded-lg border px-4 text-xs font-bold uppercase tracking-wider transition-all cursor-pointer min-w-[130px] border-0',
                  copied
                    ? 'bg-emerald-500/20 text-emerald-300 border border-emerald-500/30 shadow-[0_0_12px_rgba(16,185,129,0.2)]'
                    : 'bg-gradient-to-r from-violet-600 to-indigo-600 hover:from-violet-500 hover:to-indigo-500 text-white shadow-lg shadow-violet-950/20 border border-violet-400/20 active:scale-[0.98]',
                ].join(' ')}
              >
                {copied ? <Check size={14} className="text-emerald-400 animate-pulse" /> : <Copy size={14} />}
                {copied ? 'Copied!' : 'Copy to Agent'}
              </button>
            </div>

            <pre className="max-h-48 overflow-y-auto p-4 font-mono text-[10px] leading-relaxed text-white/60 bg-black/20 select-all scrollbar-thin">
              {activePrompt.body}
            </pre>
          </div>

          {copyError && (
            <div className="rounded-lg border border-rose-500/30 bg-rose-500/10 px-3 py-2 text-[11px] text-rose-300">
              {copyError}
            </div>
          )}
        </div>

        {/* Dismissal Section */}
        <div className="mt-2 flex items-center justify-between gap-4 border-t border-white/10 pt-4 bg-transparent">
          <label className="flex cursor-pointer items-center gap-2 text-[11px] text-white/50 hover:text-white/70 select-none">
            <input
              type="checkbox"
              checked={dontShow}
              onChange={(e) => setDontShow(e.target.checked)}
              className="h-3.5 w-3.5 cursor-pointer accent-violet-500 bg-black/40 border border-white/10 rounded"
            />
            Don’t show this on startup
          </label>
          <button
            onClick={handleClose}
            className="h-9 shrink-0 cursor-pointer rounded-lg bg-gradient-to-r from-violet-600 to-indigo-600 hover:from-violet-500 hover:to-indigo-500 text-xs font-semibold text-white shadow-lg shadow-violet-950/40 border border-violet-400/20 active:scale-[0.98] px-5 transition-all"
          >
            Got it, Let's Code!
          </button>
        </div>
      </div>
    </div>
  )
}

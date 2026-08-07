# Context Engineering Prompt Architecture (CEPA)

A guide for developers pointing an AI agent at a Repo Graph workspace.

The problem CEPA solves is not "the model is not smart enough". It is that agents
burn their context window re-reading files they did not need. On the reference
query in [`BENCHMARK_REPORT.md`](./BENCHMARK_REPORT.md) §2, the naive approach —
read the store and every component that touches it — costs **27,620 input
tokens**. The same question answered through a lean tool sequence costs **398**.

CEPA is the discipline that produces the second number: treat the context window
as limited short-term memory, and curate exactly what enters it each turn.

> **Companion docs:** [`PLAYBOOK.md`](./PLAYBOOK.md) §26 is the normative spec.
> [`BENCHMARK_REPORT.md`](./BENCHMARK_REPORT.md) has the measurements.
> [`AGENT_CONTEXT_ARCHITECTURE.md`](./AGENT_CONTEXT_ARCHITECTURE.md) documents the
> wire formats. [`../my-agent/RULES.md`](../my-agent/RULES.md) holds the hard
> constraints agents must obey.

---

## 1. The Seven-Piece Context Stack

Partition everything you put in front of the model into these seven layers. The
value is in the partitioning: when a turn goes wrong you can point at the layer
that was wrong, and when context runs short you know which layer to cut.

| # | Layer | In a Repo Graph workspace | Typical size |
| --: | :--- | :--- | ---: |
| 1 | **Instructions** | Core guardrails: no blind `grep`/glob, prefer `repograph_explore`. Lives in [`../my-agent/RULES.md`](../my-agent/RULES.md) and [`../CLAUDE.md`](../CLAUDE.md). | ~1–2k tokens, stable |
| 2 | **User Input** | The actual task. One paragraph, not a transcript. | ~100 tokens |
| 3 | **Retrieved Facts** | Verbatim tool output — symbol signatures and edge maps from `repograph_explore`. Never paraphrased, never guessed. | **~400 tokens if lean, ~27k if careless** |
| 4 | **Tools** | Schemas for `repograph_explore`, `repograph_search`, `repograph_node`, `repograph_files`, `repograph_status`, `repograph_callers`, `repograph_callees`, `repograph_impact`. | ~500 tokens, stable |
| 5 | **Short-term Notes** | Running checklist in [`../my-agent/memory/runtime/context.md`](../my-agent/memory/runtime/context.md) — state kept *outside* the window. | ~200 tokens |
| 6 | **Long-term Memory** | Project constraints and style: [`UI_UX_DESIGN_SYSTEM.md`](./UI_UX_DESIGN_SYSTEM.md), [`PLAYBOOK.md`](./PLAYBOOK.md). Loaded **selectively**, never wholesale. | load on demand |
| 7 | **Output Format** | The response schema you require — a diff, a JSON payload, a table. | ~100 tokens |

**Layer 3 is the only one that scales with repo size.** Layers 1, 2, 4, 5 and 7
are roughly constant no matter how large the codebase is. So every token you
save is saved in layer 3, which is exactly what the discovery sequence below
optimizes. Tuning the wording of your system prompt is rearranging deck chairs
by comparison.

---

## 2. The Three-Step Discovery Sequence

Run these in order. Each step narrows the search space so the next one stays cheap.

### Step 1 — Orient (structure)

```jsonc
{"name": "repograph_status", "arguments": {}}
{"name": "repograph_files",  "arguments": {"scope": "src/components"}}
```

`repograph_status` confirms the index is live and the active root matches the
directory you think you are in. `repograph_files` returns a compressed map —
files, exports, routes, dependency edges — not source. Pass `scope` to narrow it;
an unscoped manifest on a large repo is itself a context cost.

Its footer reports the payload format version (`payload format v1.1.0`).

### Step 2 — Target (search)

```jsonc
{"name": "repograph_search", "arguments": {"query": "store"}}
```

Symbol names are indexed with their camelCase and snake_case sub-words, so
`"store"` matches `useGraphStore` and `"auth"` matches `AuthService`. Search
matches token **prefixes**, so `"raph"` will not find `useGraphStore` — search
for a word, not a fragment.

Note that `repograph_search` returns the **full source** of every match. It is a
locator, not a reader: use it to learn *which* symbols exist, then stop.

### Step 3 — Explore leanly

```jsonc
{"name": "repograph_explore",
 "arguments": {"symbols": ["useGraphStore"], "signature_only": true}}
```

Returns each symbol's declaration head plus its collapsed call graph.
`compact_edges` defaults to the value of `signature_only`, so this single flag
gets you the smallest faithful payload — **1,473 characters** for the reference
query, versus 12,679 with bodies:

```
export const useGraphStore = create<GraphState>((set, get) => (

src/components/GraphCanvas.tsx -references-> src/store.ts#useGraphStore
src/components/nodes/CustomFileNode.tsx#CustomFileNode -calls-> src/store.ts#useGraphStore
src/store.ts#useGraphStore -calls-> src/lib/hoverHighlight.ts#applyHoverHighlight
… 17 edges total
```

`start_line` / `end_line` still report each symbol's true span, so the agent can
see how much body it skipped and decide whether it needs it.

### Step 4 — Load bodies, but only to write

```jsonc
{"name": "repograph_explore", "arguments": {"symbols": ["useGraphStore"]}}
```

**Never edit code from a signature.** The moment the task turns from
understanding to modifying, drop `signature_only` and read the implementation.
The savings exist to buy you room for the bodies that actually matter — not to
make the agent guess.

| Phase | Call | Cost (reference query) |
| :--- | :--- | ---: |
| Orient | `repograph_files(scope)` | manifest only |
| Target | `repograph_search(query)` | locator |
| Explore | `repograph_explore(signature_only: true)` | **398 tokens** |
| Write | `repograph_explore(...)` full body | 3,427 tokens |
| *(naive)* | read 7 files in full | *27,620 tokens* |

---

## 3. Copy-pasteable prompt headers

### 3.1 Minimal header (drop into any agent's system prompt)

```markdown
This workspace is indexed by Repo Graph (MCP server `repo-graph`).

Do not grep, glob, or read files at random. Follow this sequence:
1. ORIENT  — repograph_status, then repograph_files(scope) for the area in question.
2. TARGET  — repograph_search(query) to find candidate symbol names.
3. EXPLORE — repograph_explore(symbols, signature_only: true) for declarations
             and the call graph. This is the default for any question about
             structure, dependencies, or "what uses what".
4. WRITE   — only when you are about to modify code, re-call
             repograph_explore(symbols) without signature_only to load bodies.

Never write code against a signature you have not expanded.
Empty results ({"files":[],"paths":[]} or []) mean "no match", not "error" —
refine the query before concluding anything.
```

### 3.2 Architecture-question header

```markdown
Answer using Repo Graph only; do not read whole files.

repograph_explore(symbols: ["<symbol>"], signature_only: true)

Report: (a) the declaration, (b) which files reference it, (c) what it calls.
In the edge list, `-calls->` is a named caller and `-references->` is a file
whose references were all local bindings. Cite file paths from the edges;
do not infer callers that are not in the payload.
```

### 3.3 Refactor / impact header

```markdown
Before changing <symbol>:
1. repograph_impact(path: "<file>", symbol: "<symbol>") for the blast radius.
2. repograph_explore(symbols: ["<file>#<symbol>"]) — full body, you are editing.
3. For each impacted file, repograph_explore(signature_only: true) first;
   expand to full bodies only for the ones you will actually change.

Always pass BOTH path and symbol to callers/callees/impact — a bare symbol
silently returns "No callers found" even when callers exist.
```

### 3.4 Task-kickoff template (the full seven-piece stack)

```markdown
# Role: <role>

## 1. INSTRUCTIONS
<guardrails — reference ../my-agent/RULES.md rather than restating it>

## 2. RETRIEVED FACTS
<paste verbatim repograph_explore output — never paraphrase tool results>

## 3. ACTIVE USER INPUT
<the task, in one paragraph>

## 4. SHORT-TERM NOTES
Track the checklist in my-agent/memory/runtime/context.md; do not keep
intermediate state in the reply.

## 5. OUTPUT FORMAT
<diff / table / JSON schema>
```

---

## 4. Managing context across turns

Four moves keep a long session from degrading (from the context-engineering
framework this architecture is built on):

1. **Write** — park checklists and open questions in
   [`../my-agent/memory/runtime/context.md`](../my-agent/memory/runtime/context.md),
   not in the conversation.
2. **Select** — retrieve only what the current turn needs. `signature_only` is
   this principle expressed as a tool parameter.
3. **Compress** — summarize resolved history; keep decisions and their reasons,
   drop the transcript that produced them.
4. **Isolate** — split large jobs into separate agents (a "reader" that maps the
   code, a "writer" that changes it) so neither inherits the other's noise.

---

## 5. When CEPA does *not* apply

Honest limits, all measured — see [`BENCHMARK_REPORT.md`](./BENCHMARK_REPORT.md) §5:

- **`explore` can cost more than a plain read.** For `buildFlow`, which spans
  75% of its file, full-body `explore` measured **1.31× worse** than reading the
  file. Slicing only pays when there is bulk to discard. (`signature_only`
  sidesteps this — a declaration head is small regardless of the ratio.)
- **The 98.56% figure is one query, not a property of the tool.** Full-body
  retrieval of the same symbol measures 87.59%. Savings are per-question. Do not
  quote a number you have not measured for the question you are asking.
- **Signatures cannot answer behavioural questions.** "Why does this return
  undefined?" needs the body. Use the sequence to find *which* body, then load it.
- **The index can be stale.** If a tool response carries the
  `⚠️ files edited since last index sync` banner, re-read those files before
  editing. A signature from a stale index is worse than no signature.
- **`repograph_search` is a locator, not a reader** — it returns the full source
  of every match, so a broad query is expensive. Narrow it, then explore.

---

## 6. Quick reference

| Question type | Call | Why |
| :--- | :--- | :--- |
| "What is in this repo?" | `repograph_files(scope)` | Manifest substitution — where the largest savings live |
| "Where is X?" | `repograph_search("x")` | Sub-word tokenized; matches prefixes |
| "What uses X?" | `repograph_explore([X], signature_only: true)` | Declaration + collapsed call graph, ~400 tokens |
| "How does X work?" | `repograph_explore([X])` | Full body — you need the logic |
| "What breaks if I change X?" | `repograph_impact(path, symbol)` | Transitive dependents; **pass both args** |
| "Read this whole file" | `repograph_node(path)` | Cheaper than `explore` when the symbol dominates its file |

Registered in [`../my-agent/RULES.md`](../my-agent/RULES.md) under
`CONTEXT_ENGINEERING_PROMPT_ARCHITECTURE_MARKER`, so any agent connecting to this
workspace picks it up automatically.

# Context Cost & Token Savings Benchmark Report

> **Methodology change (2026-07-26): token counts are now exact, not estimated.**
> Every figure in §2 was originally derived from `chars / 3.7`. Counting is now
> done with a real BPE tokenizer (`repo_graph::tokens`, `o200k_base`), and the
> re-measured numbers are in **§2.3**. The headline savings *rose* — the old
> estimate read ~9% high on this baseline, so it understated the win.
>
> The arc table in §2 is kept as originally published, marked as estimated,
> because tiers 1–3 measure code paths that no longer exist and cannot be
> honestly re-run. §2.3 is the number to quote.

## 1. Executive Summary
The **Repo Graph** system replaces file-level crawling and full-text searches (Arm A) with high-fidelity, symbol-level context extraction (Arm B). Payload format **v1.1.0** adds two response modes — `signature_only` and `compact_edges` — that answer architecture-level questions with declarations and a call graph instead of function bodies, reaching **99.07%** input token context savings on the reference query below (§2.3, exact BPE counts).

Read §2 as an *arc*, not a single number: three of the five tiers were measured on the same query as the naive implementation was fixed, and §5 documents a real case where `explore` costs **more** than a plain read. Quote the tier that matches the question being asked.

---

## 2. Payload Optimization Arc (v1.1.0)

**Reference query:** *"How is global state managed in this repository, which store holds visualizer state, and what components subscribe to or update it?"*

**Arm A baseline** — reading the store plus its 6 subscriber components in full
(`src/store.ts`, `App.tsx`, `GraphCanvas.tsx`, `LeftSidebar.tsx`, `DetailSidebar.tsx`, `CustomFileNode.tsx`, `CustomSymbolNode.tsx`):
**102,193 chars / 27,620 tokens / \$0.082860**.

**Arm B** — `repograph_explore(["useGraphStore"])`, measured over stdio JSON-RPC by byte-counting the tool result:

| Tier | Mode | Chars | Tokens | Cost (USD) | Savings |
| :--- | :--- | ---: | ---: | ---: | ---: |
| 0 | Arm A baseline (7 files read in full) | 102,193 | 27,620 | \$0.082860 | — |
| 1 | Original `explore` (pre-pruning) | 26,891 | 7,268 | \$0.021804 | 73.69% |
| 2 | Pruned `paths` (edge collapsing) | 12,679 | 3,427 | \$0.010281 | 87.59% |
| 3 | `compact_edges` alone (full bodies) | 12,067 | 3,261 | \$0.009783 | 88.19% |
| 4 | `signature_only` (verbose edges) | 2,085 | 564 | \$0.001692 | 97.96% |
| 5 | **`signature_only` + `compact_edges`** | **1,473** | **398** | **\$0.001194** | **98.56% ✅** |

*USD at \$3.00 per million input tokens; tokens **estimated** at 3.7 chars/token.
Superseded by the exact counts in §2.3 — this table is retained because tiers
1–3 measure code that has since been replaced.*

### 2.3 Re-measured with an exact tokenizer

Produced by `src-tauri/tests/benchmark_remeasure.rs` (run with `--nocapture`),
so these numbers regenerate on demand rather than being transcribed by hand:

| Arm | Chars | Tokens (exact BPE) | Tokens (old estimate) | Estimate error | Cost (USD) |
| :--- | ---: | ---: | ---: | ---: | ---: |
| Arm A — 7 files read in full | 119,040 | **29,452** | 32,173 | +9.2% | \$0.088356 |
| Tier 5 — `signature_only` + `compact_edges` | 1,053 | **275** | 285 | +3.6% | \$0.000825 |
| **Savings** | | **99.07%** | 99.11% | | **107× cheaper** |

Three things worth reading off this table:

1. **The claim got stronger, not weaker.** 98.56% → **99.07%**. The old divisor
   overstated the baseline by 9.2%, which flattered the *denominator* and so
   understated the saving.
2. **Arm A is larger than the 102,193 chars first published** because those
   seven files have since grown. The baseline is re-read from the working tree
   on every run rather than pinned, so it tracks the repo instead of drifting
   out of date.
3. **Switching estimators moved the ratio by 0.001 percentage points.** That is
   measured (`ratios_are_more_robust_than_absolute_counts`), and it is why the
   original percentages held up despite the crude estimator: per-character
   error largely cancels between numerator and denominator. Absolute token and
   dollar figures did *not* hold up, and those are the ones that were wrong.

### 2.1 Where the remaining 1,473 characters go

| Section | Chars | Content |
| :--- | ---: | :--- |
| `files` | 191 | one declaration line — `export const useGraphStore = create<GraphState>((set, get) => (` |
| `paths` | 1,263 | 17 call-graph edges as arrow strings |
| JSON envelope | 19 | object/array punctuation |

Nothing left is overhead — the edges *are* the answer to "what subscribes to it". Tier 5 is the floor for this query short of returning less information.

### 2.2 Compact edge syntax

`compact_edges` replaces `{"from_symbol": "...", "to_symbol": "...", "kind": "..."}` with a single arrow string:

```
from_symbol -kind-> to_symbol
```

Measured examples:

```
src/components/GraphCanvas.tsx -references-> src/store.ts#useGraphStore
src/components/nodes/CustomFileNode.tsx#CustomFileNode -calls-> src/store.ts#useGraphStore
src/store.ts#useGraphStore -calls-> src/lib/hoverHighlight.ts#applyHoverHighlight
```

The per-edge JSON keys cost 44 chars each — 748 of the 1,875 `paths` chars at tier 4. The `kind` stays *inside* the arrow deliberately: `calls` marks a named caller, `references` marks a file whose only references were in-function local bindings (see §6). Dropping it would save ~10 chars per edge by deleting information.

**Defaults:** `compact_edges` defaults to the value of `signature_only`, so `signature_only: true` alone already returns tier 5 (1,473 chars). Set `compact_edges: false` to keep object form, or `compact_edges: true` alone to compact a full-body response (tier 3).

**Format version:** these response shapes are payload format **v1.1.0**, reported in the `repograph_files` manifest footer (`## Agent Instructions (payload format v1.1.0)`). It is deliberately **not** the same field as `Graph::schema_version`, which is an integer identifying the on-disk `.repograph/graph.json` cache layout and remains `1` — that layout did not change.

---

## 3. Methodology

### Token counting

Tokens are counted with a real byte-pair encoder — `o200k_base`, via
`tiktoken-rs`, wrapped in `repo_graph::tokens::count_tokens`.

**This is an explicit proxy, and the limitation is stated rather than hidden.**
Anthropic ships no local tokenizer, and this tool is offline-first by design
(`CLAUDE.md` §3), so a byte-exact Claude count is not obtainable here.
`o200k_base` is a modern large-vocabulary BPE, embedded in the crate (nothing
is fetched at runtime) and deterministic across machines. Read the figures as
"tokens, measured with a real BPE", not as Claude's exact billing count.

**Why the previous `chars / 3.7` estimate had to go.** A single divisor is not
merely imprecise, it is wrong in *opposite directions* depending on content, so
its error cannot be reasoned about — only measured:

| Content | Actual chars/token | `chars / 3.7` error |
| :--- | ---: | ---: |
| English prose | 5.04 | **+37%** (reads high) |
| Dense Rust | 3.38 | −9% (reads low) |
| TypeScript | 3.73 | ~0% |
| Manifest line | 3.61 | −4% |
| Compact arrow edge | 3.75 | ~0% |

The payloads this tool actually emits sit close to 3.7, which is why the
published *ratios* survived the switch. That was luck, not method — and it did
not save the absolute token and cost figures, which were off by ~9%.

`estimate_tokens_legacy` is retained in `tokens.rs` solely so the historical
§2 table stays reproducible.

### Formulas

### Ingestion Cost
$$Cost = T \times \frac{\$3.00}{1,000,000}$$
Where \(T\) is the exact input token count.

### Token Context Savings Percentage
$$\text{Savings } \% = \left(1 - \frac{T_{\text{RepoGraph}}}{T_{\text{Baseline}}}\right) \times 100$$
Using the re-measured tier-5 metrics from §2.3:
$$\text{Savings } \% = \left(1 - \frac{275}{29{,}452}\right) \times 100 = \mathbf{99.07\%}$$

---

## 4. Architectural Analysis

Repo Graph achieves these context savings through five architectural advancements:
1. **Dynamic AST Slicing (`explore`):** Rather than feeding entire source files to the LLM context, `explore` queries the SQLite database to identify the precise `start_line` and `end_line` ranges of target symbols, reading only the relevant snippet (e.g. 50 lines for `open_in_editor` instead of 520 lines in full).
2. **Batch Call-Graph Walks:** The `explore` tool retrieves both the symbol source code slices and their callers/callees in a single round-trip, avoiding multiple sequential tool invocations.
3. **Structured Subgraph Manifests (`repograph_files`):** Manifest output is generated as a compressed codebase map containing only files, routes, exports, and imports, providing a lightweight index of the repository.
4. **Edge Collapsing (v1.1.0):** Raw call-graph rows are mostly in-function locals — 111 of 138 for `useGraphStore`, plus 12 `file#file` pseudo-edges. Named symbols are kept; a file contributing only locals collapses to one `references` edge instead of being dropped, because for a React store *every* subscriber appears only as a local binding. §2 tier 2.
5. **Signature Mode (v1.1.0):** `signature_only` returns each symbol's declaration head — from its first line to the line that opens the body (`{`, `:`, `=>`, `;`), capped at 6 lines — while `start_line`/`end_line` still report the symbol's true span so an agent can see how much body it skipped. §2 tiers 4–5.

---

## 5. Measured Counter-Case: When `explore` Costs *More* Than a Full Read

The §2 figures were measured on a symbol that is a **small fraction of the total context an agent would otherwise read**. That is the favourable case, not the universal one. Full-body slicing only pays when there is bulk to discard — and this counter-case is exactly what motivated `signature_only`.

Measured directly against this repository (`buildFlow` in `src/lib/layout.ts`):

| Approach | Chars | Est. tokens |
| :--- | ---: | ---: |
| `repograph_explore(["buildFlow"])` — 4,614 escaped code + 3,100 `paths` | **7,834** | **~2,117** |
| Plain full read of `src/lib/layout.ts` | 5,959 | ~1,611 |
| **Delta** | **+1,875** | **+506 (1.31× worse)** |

`buildFlow` spans lines 33–169 of a 195-line file — **75% of the file** — so slicing recovers almost nothing, while the 30-edge `paths` array adds 3,100 characters of pure overhead (≈24 of those 30 edges are in-function local variables, not real calls).

**Since v1.1.0 both halves of that counter-case are addressed:** edge collapsing removes the local-variable noise, and `signature_only` makes the symbol-to-file ratio irrelevant — a declaration head is small regardless of how much of the file its body occupies. The measurement above is retained because the *default* (full-body, verbose edges) still behaves this way.

### Rule of Thumb (Symbol-to-File Ratio)

| Situation | Use | Why |
| :--- | :--- | :--- |
| "What talks to what" / architecture mapping | `repograph_explore(signature_only: true)` | Declarations + call graph; the symbol-to-file ratio stops mattering (§2 tier 5) |
| Symbol is a small part of a large file (<30% of file) | `repograph_explore` | Slicing discards the bulk; savings approach the §2 tier 2 figures |
| You need callers/callees regardless | `repograph_explore` | Call graph arrives in the same round-trip |
| You are about to edit the implementation | `repograph_explore` (full body) | Never edit from a signature — fetch the body first |
| Symbol dominates its file (>70% of file) | `repograph_node(path)` | Full-body `explore` adds `paths` overhead on top of near-identical code |
| Repo-wide orientation | `repograph_files` | Manifest substitution is where the >90% savings genuinely live |

> **Reporting note:** Savings percentages are per-query, not a property of the tool. The 99.07% headline is **`signature_only` + `compact_edges` on an architecture question** (§2.3), where bodies are pure cost. A full-body retrieval of the same symbol measures ~88%, and §5 shows a case that measures *worse than a plain read*. Quote the tier that matches the question, and measure before quoting a new one — `cargo test --test benchmark_remeasure -- --nocapture` regenerates the table.

### Cache Growth Warning

During this audit `.repograph/graph.db` reached **664 MB for a 136-file repository** (versus 448 KB for the equivalent `graph.json`), at which point `mcp_server` **hung indefinitely on startup reconciliation** — over 120 s with no output.

**Diagnosed cause (fixed).** It was not duplicate rows — row counts were normal (137 files, 4,374 FTS rows). `dbstat` showed `symbols_fts_content` alone holding **470 MB across those 4,374 rows (~110 KB each)**, and the largest entries were all single-letter symbols (`un`, `n`, `t`, `r`) inside `dist/assets/index-<hash>.js`. The file watcher's `is_supported_file` kept a private denylist naming only `.git`, `node_modules`, and `.repograph` — it did **not** include `dist`, so every `npm run build` made the watcher re-parse the minified production bundle. Minified code is a handful of enormous lines, so each of its thousands of symbols stored a ~190 KB source slice in the FTS index. A full `mcp_server index` never had this problem because the *walker* always skipped `dist`; only the incremental watcher path was affected.

The watcher now shares the walker's `SKIP_DIRS` and `language_for` (regression tests: `build_output_is_never_re_indexed`, `real_sources_are_watched_including_newly_supported_languages`). A fresh index of this repo now produces **0.37 MB** for 139 files in 343 ms.

If you are on a cache built before this fix, delete `.repograph/graph.db` and re-index — the fix prevents new bloat but does not shrink an already-bloated file.

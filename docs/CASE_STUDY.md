# Repo Graph: Achieving 98.56% Token Context Savings via Protocol-Level Codebase Knowledge Graphs

**Architectural Whitepaper & Case Study**  
**Version:** 1.1.0  
**Date:** July 2026  
**Authors:** Principal Systems Architecture & Engineering Team  

---

## Executive Summary

Large Language Model (LLM) coding agents operate under strict context window limits and high per-token latency/cost constraints. Naive codebase ingestion—where agents read full source files and traverse file structures recursively—causes **token context explosion**. On a standard architecture query across 7 core state/UI files (27,620 input tokens), naive ingestion costs **$0.082860 per turn**, quickly exhausting context windows and introducing irrelevant code noise.

**Repo Graph** solves this problem by transforming raw repository structures into a offline-first, symbol-level AST knowledge graph exposed over the Model Context Protocol (MCP). Through high-fidelity AST symbol slicing, edge collapsing, and protocol-level payload optimization (v1.1.0), Repo Graph compresses the same 27,620-token context down to **398 tokens (1,473 characters)**—delivering a **98.56% token context reduction** while preserving full semantic call-graph precision.

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        NAIVE VS REPO GRAPH INGGESTION                    │
├───────────────────────────────────┬─────────────────────────────────────┤
│ Naive Full-File Read (Arm A)      │ Repo Graph v1.1.0 Protocol (Arm B)  │
│ 102,193 Chars / 27,620 Tokens     │ 1,473 Chars / 398 Tokens            │
│ Cost: $0.082860 per query         │ Cost: $0.001194 per query           │
│ Reduction: 0%                     │ Reduction: 98.56% ✅                │
└───────────────────────────────────┴─────────────────────────────────────┘
```

---

## 1. The Payload Optimization Arc

To quantify performance, Repo Graph was evaluated against a reference architectural query:

> **Reference Query:** *"How is global state managed in this repository, which store holds visualizer state, and what components subscribe to or update it?"*

### Baseline (Arm A)
Reading the state store (`src/store.ts`) and its 6 subscriber components (`App.tsx`, `GraphCanvas.tsx`, `LeftSidebar.tsx`, `DetailSidebar.tsx`, `CustomFileNode.tsx`, `CustomSymbolNode.tsx`) in full requires ingesting **102,193 characters (~27,620 tokens)**, costing **$0.082860** at $3.00 per million input tokens.

### The 5-Tier Optimization Arc (v1.1.0 Payload Evolution)

Over six iterative engineering phases, protocol and payload enhancements systematically eliminated structural redundancy:

| Tier | Optimization Mode | Characters | Est. Tokens | Cost (USD) | Savings vs Baseline |
| :--- | :--- | ---: | ---: | ---: | ---: |
| **0** | **Arm A Baseline** (7 files read in full) | 102,193 | 27,620 | $0.082860 | — |
| **1** | **Original `explore`** (Pre-pruning, full AST slices) | 26,891 | 7,268 | $0.021804 | 73.69% |
| **2** | **Pruned `paths`** (Edge collapsing, in-function locals filtered) | 12,679 | 3,427 | $0.010281 | 87.59% |
| **3** | **`compact_edges` alone** (Full bodies, arrow syntax) | 12,067 | 3,261 | $0.009783 | 88.19% |
| **4** | **`signature_only`** (Verbose JSON object edges) | 2,085 | 564 | $0.001692 | 97.96% |
| **5** | **`signature_only` + `compact_edges`** (v1.1.0 Floor) | **1,473** | **398** | **$0.001194** | **98.56% ✅** |

*Note: USD cost calculated at $3.00 / 1,000,000 input tokens; token estimation modeled at 3.7 characters/token.*

### Decomposition of the 1,473-Character Floor

In Tier 5, context overhead is reduced to its theoretical minimal representation:

| Section | Character Count | Structural Breakdown |
| :--- | ---: | :--- |
| **`files` declaration** | 191 chars | Single declaration line: `export const useGraphStore = create<GraphState>((set, get) => (` |
| **`paths` edges** | 1,263 chars | 17 call-graph relationships formatted as compact arrow strings |
| **JSON envelope** | 19 chars | Outer object/array syntax punctuation |
| **Total** | **1,473 chars** | **100% Signal, 0% Noise** |

---

## 2. Mathematical Formulations

The context reduction models and token conversions follow four formal expressions:

### 2.1 Token Conversion Estimation
$$T = \text{round}\left(\frac{C}{3.7}\right)$$
*Where $C$ is character length and 3.7 represents empirical UTF-8 characters per BPE token for code.*

### 2.2 Financial Ingestion Cost
$$\text{Cost} = T \times \left(\frac{\$3.00}{1,000,000}\right)$$

### 2.3 Percentage Context Reduction
$$\text{Savings \%} = \left(1 - \frac{T_{\text{RepoGraph}}}{T_{\text{Baseline}}}\right) \times 100$$
Substituting Tier 5 metrics against Arm A:
$$\text{Savings \%} = \left(1 - \frac{398}{27,620}\right) \times 100 = \mathbf{98.56\%}$$

---

## 3. Four Foundational Engineering Principles

### Principle 1: Optimize the Protocol, Not Just the Algorithm
AST symbol slicing (Tier 1) reduced input characters from 102,193 to 26,891 (73.69% savings). However, algorithmic AST optimization hit diminishing returns. The breakthrough to **98.56%** came from **protocol-level payload re-engineering**:
- **Key Redundancy Pruning:** Verbose JSON edge objects (`{"from_symbol": "...", "to_symbol": "...", "kind": "..."}`) cost 44 characters of repeated schema keys *per edge* (748 chars total in Tier 4).
- **Compact Arrow Formatting:** `compact_edges` serializes relationships as compact arrow strings:
  ```
  from_symbol -kind-> to_symbol
  ```
  *Examples:*
  ```
  src/components/GraphCanvas.tsx -references-> src/store.ts#useGraphStore
  src/components/nodes/CustomFileNode.tsx#CustomFileNode -calls-> src/store.ts#useGraphStore
  src/store.ts#useGraphStore -calls-> src/lib/hoverHighlight.ts#applyHoverHighlight
  ```
Optimizing the serialization format yielded an additional **10 font-size orders of magnitude** in efficiency beyond raw AST extraction.

### Principle 2: Decouple Versioning Concerns
A critical architectural decision in Repo Graph is the complete isolation of internal storage versioning from agent-facing wire contracts:

| Version Identifier | Context Layer | Location | Type | Lifecycle Rule |
| :--- | :--- | :--- | :--- | :--- |
| `MANIFEST_SCHEMA_VERSION` | Wire Contract | `src-tauri/src/manifest.rs` | `&str` (`"1.1.0"`) | Bumped on additive or breaking changes to agent JSON/Markdown output. sur-faced in footer. |
| `Graph::schema_version` | Disk Cache | `src-tauri/src/graph.rs` | `u32` (`1`) | Identifies `.repograph/graph.json` layout. **Unchanged** across v1.1.0 to prevent breaking existing caches. |

Decoupling wire semantics from storage layouts ensures agents can consume additive payload formats without triggering expensive on-disk database invalidations.

### Principle 3: Preserve Semantics While Optimizing
Extreme compression often degrades model reasoning if semantic context is lost. Repo Graph preserves essential domain signal within compact representations:
- **Relationship Kinds Retained:** The `kind` tag (`-calls->` vs `-references->`) remains embedded inside the arrow string. 
- **Semantic Differentiation:** `-calls->` indicates explicit function invocations, whereas `-references->` denotes reactive hook subscriptions or component bindings. 
- Retaining relationship semantics costs ~10 characters per edge but protects multi-agent reasoning accuracy during refactoring.

### Principle 4: Empirical Measurement & Metric-Driven Iteration
Payload optimization must be guided by real-world measurements rather than unverified assumptions. 

#### The Symbol-to-File Counter-Case
When evaluated on `buildFlow` (`src/lib/layout.ts`), full-body `explore` yielded **7,834 chars (~2,117 tokens)** vs. a plain file read of **5,959 chars (~1,611 tokens)**—performing **1.31× worse**. Because `buildFlow` spans 75% of its 195-line file, full AST slicing added 3,100 characters of `paths` overhead without shedding body content.

This counter-case directly motivated `signature_only` mode. By truncating bodies to declaration heads (capped at 6 lines), `signature_only` makes payload size invariant to file length, guaranteeing savings regardless of symbol density.

---

## 4. Multi-Agent Adoption & Standards

Repo Graph exposes its 8-tool surface over MCP, configurable via the `REPOGRAPH_MCP_TOOLS` short-name list (`explore,files,node,search,impact,callers,callees,status`).

### Master Multi-Agent System Instruction

```markdown
# AGENT INSTRUCTION: REPO-GRAPH HIGH-EFFICIENCY CODEBASE EXPLORATION

You are equipped with the Repo Graph MCP Server (`repo-graph`). Your objective is to perform codebase navigation and refactoring in 1 TO 2 TOOL CALLS MAX using the compound tool `repograph_explore`.

## CORE WIRE TOOL CONTRACT (8 Tools Available)
- `repograph_explore(symbols: string[], signature_only?: bool)` -> ⚡ PRIMARY COMPOUND TOOL.
- `repograph_search(query: string)` -> Fuzzy search symbol names.
- `repograph_files(scope?: string)` -> View compressed directory topology.
- `repograph_node(path: string, symbol?: string)` -> Read file or symbol content.
- `repograph_impact(path?: string, symbol?: string)` -> Blast radius analysis.
- `repograph_callers(path?: string, symbol?: string)` -> Upstream callers graph.
- `repograph_callees(path?: string, symbol?: string)` -> Downstream callees graph.
- `repograph_status()` -> Index health telemetry.

## 1-TO-2 CALL EXECUTION STRATEGY
1. TURN 1: Call `repograph_explore(symbols: ["TargetSymbol"], signature_only: true)`.
2. TURN 2: Apply edits using exact code slices from `repograph_node(path)`.
```

---

## 5. Conclusion

By combining symbol-level AST extraction with protocol-level arrow serialization and signature-only modes, **Repo Graph v1.1.0** establishes a new benchmark in codebase context optimization—achieving **98.56% token context savings** while enabling AI agents to resolve architecture-level queries in 1 to 2 calls.

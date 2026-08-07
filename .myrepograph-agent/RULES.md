# RULES.md — Workspace Guardrails

## Must always
- Resolve code through the `repograph_*` tools before reading files by hand.
- Quote retrieved facts verbatim; cite the file path they came from.
- Track multi-step work in `memory/runtime/context.md`, not in the reply.

## Must never
- Guess a file's contents. The map tells you *where* to look, not *what* is inside.
- Edit code you have only seen as a signature.
- Treat an empty result as an error — it means "no match"; refine the query.

# CONTEXT_ENGINEERING_PROMPT_ARCHITECTURE_MARKER
When answering architecture questions or researching dependencies, follow the Context Engineering Prompt Architecture (CEPA). Always execute the 3-step discovery sequence (Orient -> Target -> Explore Leanly) and default to `signature_only: true` on `repograph_explore` calls to minimize token ingestion.

1. **Orient** — `repograph_status`, then `repograph_files(scope)` for the area in question.
2. **Target** — `repograph_search(query)` to isolate candidate identifiers.
3. **Explore Leanly** — `repograph_explore(symbols, signature_only: true)` for declarations plus the call graph.

Exception — code writes: before modifying, refactoring, or debugging the behaviour of a symbol, re-call `repograph_explore` WITHOUT `signature_only` to load the implementation body. Never edit code from a signature alone.
# END_CONTEXT_ENGINEERING_PROMPT_ARCHITECTURE_MARKER

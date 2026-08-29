<!-- BEGIN REPO-GRAPH-SYNC-POLICY v1.3 -->
# MCP Continuous Sync & Strict Token Cost-Cutting Policy

1. **Persistent Session Priority**:
   - Always prioritize calling `repo-graph` MCP tools (`repograph_explore`, `repograph_files`, `repograph_node`, `repograph_impact`, `repograph_edit`, `repograph_write`, `repograph_delete`, `repograph_batch_edit`, `repograph_edit_symbol`, `repograph_status`) instead of brute-force directory scans or native shell commands.
2. **Pre-Flight Check & Context Restoration (Start of Turn)**:
   - On the first turn of any task or session, call `repograph_status` to verify connection and confirm `Sync State` is `Synced`.
   - Read `.myrepograph-agent/memory/runtime/context.md` to restore working context, active task goals, and previously resolved symbol references without wasting tokens re-indexing.
3. **Strict Sliced Reading (No Full-File Dumps)**:
   - Prohibit reading full files (>50 lines) when only a specific function, type, or block is required.
   - Always perform targeted sliced reads using `repograph_node(path="...", start_line=N, end_line=M, with_line_numbers=true)`.
4. **Strict Scoped Discovery (Signature-First Protocol)**:
   - Never ingest entire directory trees; always supply `scope` (e.g. `repograph_files(scope="src/lib/**")`).
   - Ingest signatures only (`signature_only: true`) via `repograph_explore` during architecture exploration and call-tree analysis.
   - Only retrieve full implementation bodies when actively modifying or debugging the specific symbol.
5. **Strict Bounded Searches**:
   - Bound all `repograph_search` queries with `limit: 10` or `exact_symbol_only: true` to prevent oversized result payloads from polluting the context window.
6. **Closed-Loop MCP Mutation & Impact Analysis**:
   - Before modifying central interfaces, run `repograph_impact(symbol="<name>")` or `repograph_callers` to evaluate downstream ripple effects.
   - Use `repograph_edit`, `repograph_batch_edit`, and `repograph_edit_symbol` for atomic refactors with instant AST re-indexing and rollback safety.
7. **Turn Completion & Zero-Token Memory Offloading (End of Turn)**:
   - Update checklist and active goals in `.myrepograph-agent/memory/runtime/context.md` to offload working memory outside the model context window.
   - Append a session summary entry into `.myrepograph-agent/memory/runtime/dailylog.md` detailing what changed, edge diffs, verification commands executed, and any pending items.
<!-- END REPO-GRAPH-SYNC-POLICY v1.3 -->

> **Context Engineering & Token Optimization Delegation:**
> Detailed file/folder token cutting policies, transient state offloading, and behavioral rules are centrally defined in [`.myrepograph-agent/RULES.md`](file:///c:/My-pro/project-map/.myrepograph-agent/RULES.md). All agents must strictly follow these rules.

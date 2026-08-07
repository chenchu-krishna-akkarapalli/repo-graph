# Agents

Agent roles for this workspace and the context each one is allowed to load.

## Roles

| Agent | Purpose | Context budget |
| :--- | :--- | :--- |
| *coordinator* (default) | Plans work, delegates, never edits blind | Manifest + signatures |
| `agents/fact-checker` | Verifies claims against the indexed graph | Signatures only |

## The seven-piece context stack

Partition what you put in front of the model; when a turn goes wrong you
want to know which layer was wrong.

1. **Instructions** — `RULES.md` (guardrails)
2. **User Input** — the task, one paragraph
3. **Retrieved Facts** — verbatim `repograph_explore` output, never paraphrased
4. **Tools** — the `repograph_*` schemas
5. **Short-term Notes** — `memory/runtime/context.md`, kept *outside* the window
6. **Long-term Memory** — `knowledge/`, loaded selectively
7. **Output Format** — the schema you require back

Only layer 3 scales with repository size. Every token worth saving is saved
there, which is what the discovery sequence in `RULES.md` optimizes.

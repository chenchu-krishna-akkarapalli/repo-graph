# SKILL: repo-graph

## Description (trigger conditions)

Use this skill whenever an agent needs to understand the structure of a
codebase before editing it — especially in large repos where reading every
file would blow the context budget. Triggers include: "where is X defined",
"what depends on Y", "fix the login button", "add a field to the User
model", or any task that requires locating relevant files in an unfamiliar
or large project before making changes. Do NOT use this skill for trivial
single-file scripts or repos already fully visible in context.

## What this skill provides

1. **`get_manifest()`** — returns the compressed project map (see format
   below): every file, its exports, and its dependency edges. This is the
   first call an agent should make on entering a repo it hasn't mapped yet.
2. **`read_file(path)`** — returns the exact contents of a single file.
   Agents should call this only for files identified as relevant via the
   manifest — never to browse speculatively.
3. **`find_dependents(path)`** — returns every file that imports/depends on
   the given file, useful for impact analysis before a change.
4. **`find_route(pattern)`** — for web projects, resolves a URL/route pattern
   (e.g. `/api/login`) to the file(s) that implement it.

## Manifest format (what agents receive from `get_manifest()`)

```markdown
## Project Architecture Map
- /src/components/Button.tsx (Exports: Button) (Depends on: /src/hooks/useTheme.ts)
- /src/pages/api/login.ts (Route: /api/login) (Depends on: /src/lib/db.ts)

## Agent Instructions
You are looking at a compressed map. Do NOT guess file contents.
Use the tool `read_file(path)` to inspect the exact contents of a file before editing it.
```

## Usage pattern for the calling agent

1. Call `get_manifest()` once per session (or after a file-change signal).
2. Match the task description against exports/route markers in the manifest
   to shortlist candidate files — usually 1–5 files, not the whole repo.
3. Call `read_file(path)` only for the shortlisted files.
4. If a shortlisted file references another file not yet read and the task
   requires deeper context, call `find_dependents` or `read_file` again —
   don't pre-fetch everything "just in case."
5. Make the edit. Do not re-request the manifest unless the repo structure
   may have changed (new files, renamed imports).

## Non-goals

- This skill does not execute, build, or run any project code — static
  analysis only.
- This skill does not replace a full-repo review when the agent's task is
  explicitly "review the whole codebase" — it's an optimization for
  targeted edits, not a substitute for thorough audits when those are asked
  for.

## Failure modes to guard against

- **Stale manifest:** if files have changed since the last `get_manifest()`
  call, edges may be wrong. Re-index on file-watcher signal or before
  high-stakes edits.
- **Over-trusting the manifest for content:** the manifest never contains
  file bodies — only structure. Any claim about what code *does* must come
  from `read_file`, not from the manifest alone.
- **Monorepos:** multiple package roots can produce ambiguous relative-import
  resolution. Flag repos with multiple `package.json`/`Cargo.toml` roots and
  scope the manifest per package if needed.

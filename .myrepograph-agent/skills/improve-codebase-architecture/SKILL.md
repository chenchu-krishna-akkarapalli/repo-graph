---
name: improve-codebase-architecture
description: Scan a codebase for deepening opportunities, present them as a visual HTML report, and execute architectural refactoring.
---

# Improve Codebase Architecture

Surface architectural friction and propose **deepening opportunities**: refactors that turn shallow modules into deep ones. The aim is testability, locality, and AI-navigability.

This command is informed by the project's domain model and built on the shared design vocabulary from `skills/codebase-design`:
- Use exact terms: **module**, **interface**, **depth**, **seam**, **adapter**, **leverage**, **locality**.
- Never drift into "component," "service," "API," or "boundary."
- Read ADRs in `docs/adr/` before suggesting changes so you do not re-litigate settled decisions.

---

## Workflow Process

### 1. Explore Hotspots Leanly
- Decide *where* to look before scanning.
- Inspect commit history (`git log --oneline -n 20`) to locate high-churn hotspot modules.
- Use `repograph_files(scope: "<hotspot>/**")` and `repograph_impact(symbol="...")` to measure blast radius and dependency fan-out.
- Apply the **Deletion Test**: Identify pass-through wrappers where deleting the wrapper concentrates complexity rather than scattering it.

### 2. Present Candidates as a Visual HTML Report
- Generate a single self-contained HTML file in the OS temp directory (`%TEMP%/architecture-review-<timestamp>.html` or `$TMPDIR/architecture-review-<timestamp>.html`).
- Use **Tailwind CSS via CDN** for clean layout and **Mermaid JS via CDN** for side-by-side before/after dependency diagrams.
- Include for each candidate:
  - **Files:** Monospaced module paths.
  - **Problem:** Exactly one concise sentence on what hurts.
  - **Solution:** Exactly one concise sentence on what changes.
  - **Wins:** Bullet points emphasizing locality, leverage, and test surface reduction.
  - **Before / After Diagram:** Visual comparison showing shallowness collapsing into depth.
  - **Recommendation Strength:** `Strong` (emerald), `Worth exploring` (amber), or `Speculative` (slate).
- Open the HTML report in the default browser (`start <path>` on Windows, `xdg-open` on Linux, `open` on macOS) and provide the user with the absolute path.

### 3. Interactive Grilling & Decision Loop
- Once a candidate is selected:
  - Walk through constraints, dependencies, and test migration.
  - If a candidate is rejected for a load-bearing reason, offer to record it as an Architectural Decision Record in `docs/adr/`.
  - Use `repograph_batch_edit` or `repograph_edit_symbol` to execute the refactor atomically.

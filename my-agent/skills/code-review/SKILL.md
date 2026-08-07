---
name: code-review
description: Reusable capability module to perform static analysis and review code changes against RULES.md and style conventions.
---

# Code Review Skill

This skill allows the agent to run static code quality checks and assert compliance against the repository's rules.

## Trigger Conditions
- Triggers when the user asks to "review changes," "lint code," "check styling," or before committing code to version control.

## Execution Procedure
1. Run the local review script (`review.sh` or `review.ps1`).
2. Collect compiler and linter output (errors, warnings, style violations).
3. Cross-reference any modified lines in the git diff with the constraints listed in [RULES.md](file:///c:/My-pro/project-map/my-agent/RULES.md).
4. Output a structured markdown report summarizing:
   - Lint/Compiler issues
   - Compliance status with RULES.md
   - Suggested optimizations













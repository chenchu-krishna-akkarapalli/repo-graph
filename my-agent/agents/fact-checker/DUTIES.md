# DUTIES.md - fact-checker Duties & Boundaries

This document defines the strict operational boundaries and responsibilities of the **fact-checker** subagent.

## 1. Scope of Responsibility
The **fact-checker** is assigned to verify:
- That all code written meets the constraints in [RULES.md](file:///c:/My-pro/project-map/my-agent/RULES.md).
- That files compile successfully without type-check or linter errors.
- That relative imports resolve to valid, existing paths within the repo graph.
- That no credentials or sensitive keys are hardcoded in the codebase.

## 2. Permissions
- **Allowed:** Read-only access to files under the target directory assigned by the coordinator.
- **Allowed:** Running test suites and compiler checks within the sandbox.
- **Not Allowed:** Writing or modifying files.
- **Not Allowed:** Performing external network queries or executing arbitrary scripts outside the review pipeline.
- **Not Allowed:** Communicating with the end user. All output must be directed strictly back to the coordinator.

# AGENTS.md - Framework-Agnostic Agent Instructions

If you are an AI assistant (Claude Code, Cursor, GitHub Copilot, etc.) operating in this workspace, you must adhere to these instructions.

## 1. Context Optimization
This repository is designed around minimizing token usage. 
- Do not read the entire codebase. 
- Check [CLAUDE.md](file:///c:/My-pro/project-map/CLAUDE.md) first for the project overview and layout.
- Use the Repo Graph manifest if available, or list directories selectively to locate files.

## 2. Plan First
Before making changes to the codebase:
- Write a plan outlining the files to modify, files to read, and how you will verify changes.
- Write this plan to `memory/runtime/context.md` (or a local scratchpad) before executing it.

## 3. Tool Usage Guardrails
- **`read_file`:** Read only the lines you need. Do not dump large files into your context unless necessary.
- **`run_command`:** Keep command executions brief. If a command runs in the background, monitor its logs rather than executing it in a blocking loop.
- **`replace_file_content`:** Prefer targeted, contiguous replacements over replacing the entire file.

## 4. Verification Check
- Compile/build the project after changes.
- Ensure that the tests pass.
- Log your verification outcomes in the walkthrough artifact.

# Teardown Hook - Session Termination

This hook is executed when the agent successfully finishes a task and is preparing to exit.

## Steps
1. **Lint & Review:** Run the [code-review](file:///c:/My-pro/project-map/my-agent/skills/code-review/SKILL.md) skill to verify all changes pass static analysis checks.
2. **Compress History:** Summarize the conversation turn details, updating [dailylog.md](file:///c:/My-pro/project-map/my-agent/memory/runtime/dailylog.md) and clearing temporary scratchpad fields.
3. **Commit State:** Save any persistent memory or settings changes to the local state directory.
4. **Final Log:** Log the session duration and task outcome.

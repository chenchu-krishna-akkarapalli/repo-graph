# Bootstrap Hook - Session Initialization

This hook is executed immediately when the agent begins a new coding session.

## Steps
1. **Load Context & Rules:** Read [RULES.md](file:///c:/My-pro/project-map/my-agent/RULES.md) and [CLAUDE.md](file:///c:/My-pro/project-map/CLAUDE.md) to initialize the active instructions layer.
2. **Retrieve Manifest:** Call `get_manifest()` to fetch the updated project map and update the active facts list.
3. **Verify Environment:** Check if external dependencies (e.g., node, cargo, git) are available on the path.
4. **Log Startup:** Append a startup entry to [dailylog.md](file:///c:/My-pro/project-map/my-agent/memory/runtime/dailylog.md).

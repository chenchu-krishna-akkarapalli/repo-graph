# Bootstrap Hook

Run at the start of an agent session, before the first substantive reply.

1. `repograph_status` — confirm the index is live and the active root matches
   the directory you believe you are in. Halt on mismatch.
2. Load `RULES.md`, `DUTIES.md`, `SOUL.md` into the instruction layer.
3. Read `memory/runtime/context.md` for unfinished work from the last session.
4. `repograph_files(scope)` for the area named in the task — not the whole repo.

Do not read source files in this phase. Bootstrap establishes *where* things
are; retrieval of *what* they contain belongs to the task itself.

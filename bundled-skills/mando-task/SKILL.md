---
name: mando-task
description: Create a Mando task directly from a Claude Code session. Takes a brief description of what to build or fix, researches the codebase, and queues a well-structured task for captain. Use when you realize mid-session that a separate task needs doing.
---

The user provides a brief description as the skill argument. If no argument, ask what needs doing.

1. **Research**: search the codebase (Grep/Glob) to find relevant files, patterns, and existing behavior related to the request.

2. **Resolve project**: run `mando project list`, match against the current repo (`git remote get-url origin`).

3. **Create task**: run `mando todo add "<title>" -p <project> --context "<context>"` -- 2-3 sentences describing what to build or what's wrong, with enough detail for a worker to start.

This creates a NEW task -- it does not hand off the current session.

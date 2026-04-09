---
name: mando-task
description: Create a Mando task directly from a Claude Code session. Detects project from git remote, takes a title and optional context, and queues the task for captain with its own worktree. Use when you realize mid-session that a separate task needs doing.
---

## Workflow

1. **Detect project** -- run `git remote get-url origin` in the current working directory. Extract the repo name (e.g., `mando` from `github.com/tribedotrun/mando`). This becomes the `--project` flag.

2. **Parse input** -- the user provides a title as the skill argument. If no argument, ask for a title.

3. **Gather context** (optional) -- if the user described the task in conversation, summarize the relevant context into 2-3 sentences. This becomes the `--context` flag.

4. **Create the task** -- run:
   ```bash
   mando todo add "<title>" --project <project> --context "<context>"
   ```
   If `mando` CLI is not on PATH, fall back to direct API call:
   ```bash
   curl -s -X POST \
     -H "Authorization: Bearer $(cat ~/.mando/auth-token)" \
     -H "Content-Type: multipart/form-data" \
     -F "title=<title>" \
     -F "project=<project>" \
     -F "context=<context>" \
     "http://127.0.0.1:$(cat ~/.mando/daemon.port)/api/tasks/add"
   ```

5. **Report** -- show the task ID and confirm it was queued. One line, no fluff.

## Notes

- This creates a NEW task with its own worktree. It does NOT hand off the current session.
- If the daemon is not running, the command will fail. Tell the user to start Mando first.
- Never include AI attribution in the task title or context.

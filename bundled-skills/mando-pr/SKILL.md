---
name: mando-pr
description: Prepare branch, verify wiring, run quality gate, push, summarize PR, request reviews, address feedback, and clean up. Use when ready to open or finish a pull request — NOT for intermediate commits.
---

## Step 1 — Prepare branch

**Do**

- If on `main`, create a feature branch first.
- Rebase onto `origin/main` and resolve conflicts.
- If the branch name is generic, rename to `<type>/<kebab-summary>`. **Skip renaming** if a PR already exists (renaming after PR creation closes it), the branch starts with `mando/` (captain-managed branches stay as-is), or it follows another project-managed naming convention.

**Stop if**

- Rebase conflicts need user input.

## Step 2 — Verify wiring, quality gate & push

**Do**

**Verify everything is wired** — review the diff against the merge base:

- Every new public API, route, component, config field, or command must reach a user-facing entry point — called, registered, rendered, or read.
- Fix dangling work before pushing; flag intentional gaps to surface in Step 3's **Wiring** checklist.

**Quality gate** — run the project's full gate (`mando-dev check` in Mando). Fix every failure. Iterate until gate passes with no warning or error.

**Commit & push** any remaining work.

## Step 3 — Summarize PR

**Do**

- Run `/mando-pr-summary`. It creates the PR if none exists and owns the body. **Do NOT** use Claude Code's built-in PR template.
- If a draft PR already exists, convert it to ready.

## Step 4 — Request reviews

**Do**

Post each trigger only if not already on the PR (idempotent):

1. `@codex review this PR`
2. `cursor review`

## Step 5 — Address feedback

**Do**

- Run the wait-aware PR status gate:

```bash
python3 ~/.claude/skills/mando-pr/pr_status.py --watch <pr_number>
```

- For each review comment: verify against project context — fix when valid, push back when wrong, reply on every thread.
- Ignore bot links to external apps (e.g. "see N more in …"); only act on findings posted inline on the PR.
- Address anything `pr_status.py` reports. Commit, push fixes, rerun until `ALL CLEAR`. Do not wrap `pr_status.py` in manual shell delay or polling commands.
- Reply on review *line* comments via `gh api repos/{owner}/{repo}/pulls/{pr}/comments` with `in_reply_to={comment_id}` (top-level `gh pr comment` won't thread). For top-level PR conversation comments (not anchored to a line), use `gh pr comment <pr> --body '...'`. A thread counts as addressed once the PR author has replied.

## Step 6 — Clean up

**Do**

- `git status` must be clean. For leftover screenshots, temp files, and build artifacts: commit referenced proof, delete, or move to `/tmp` (or similar outside the repo). Keep PR commits to current, referenced, minimal proof only.

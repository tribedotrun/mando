---
name: mando-pr
description: Commit, push, create PR, and tag AI reviewers. Use when ready to open a pull request — NOT for intermediate commits. Pass `--fast` for low-risk changes (skips internal review + monitoring loop).
---

## Arguments

- `--fast` — single-pass comment addressing, no internal review, no monitoring loop. Use only when the user explicitly passes `--fast`; never infer from context, risk, or your own judgment.

## Step 1 — Rebase, gate, push

Rebase `origin/main`, resolve conflicts (stop if user input is needed). Review the diff: every new public symbol must be reachable from a user-facing entry point — honor any project-specific surfacing rules in `CLAUDE.md` / `AGENTS.md`.

Run the project's full quality gate. Fix every failure before pushing; if a failure needs human judgment, credentials, or infra, stop and report — do not push a broken branch. Then commit and push.

## Step 2 — Create PR + run `/mando-pr-summary`

If no PR exists and the branch name is generic, rename to `<type>/<kebab-summary>` first (renaming after PR creation closes it). **Skip renaming** if the branch starts with `mando/` (captain-managed branches stay as-is) or follows another project-managed naming convention documented in `CLAUDE.md` / `AGENTS.md`.

Create the PR with an empty body, or convert an existing draft to ready. **Do NOT** use Claude Code's built-in PR template — `/mando-pr-summary` owns the body. Run it; verify the result contains `## Problem` / `## Solution`.

## Step 3 — Trigger reviews

External (idempotent — only post each if not already on the PR):

1. `@codex review this PR`
2. `cursor review`

Internal (skip if `--fast`; otherwise idempotent — cache the reviewed SHA in `/tmp/.x-pr-reviewed-${PR_NUM}`). Wait for results; do NOT background:

1. `pr-review-toolkit:code-reviewer` on the diff.
2. `pr-review-toolkit:silent-failure-hunter` if the change touches error handling.

Hold findings; address them in step 4.

## Step 4 — Address everything until merge-ready

Fix every internal-review finding from step 3, commit, push. Then loop the status check until exit 0:

```bash
python3 ~/.claude/skills/mando-pr/pr_status.py <pr_number>
```

1. `[FAIL]` CI → fix, commit, push.
2. `UNADDRESSED COMMENTS` → fix, reply per thread, commit, push.
3. `[WAIT]` → sleep 10s, re-check.
4. `ALL CLEAR` → done.

`--fast`: run the status check once, address what it surfaces in a single pass, stop.

Reply to threads via `gh api repos/{owner}/{repo}/pulls/{pr}/comments` with `in_reply_to={comment_id}` (top-level `gh pr comment` won't thread). The status script considers a thread "addressed" once the PR author has replied.

`git status` must be clean before finishing — commit or delete leftover screenshots, temp files, build artifacts.

## Notes

1. If on `main`, branch first.
2. Squash-merge convention.

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

Internal (skip if `--fast`; otherwise idempotent — cache the reviewed SHA in `/tmp/.x-pr-reviewed-${PR_NUM}`). If the cache file already contains the current PR head SHA, skip this internal-review block. Otherwise choose the branch for the current runtime, run both reviewers in that branch, and wait for results; do NOT background:

**If running in Claude Code:**

1. Run `pr-review-toolkit:code-reviewer` on the diff.
2. Run `pr-review-toolkit:silent-failure-hunter` on the diff.

**If running in Codex:**

1. Spawn a review-only Codex subagent for general code review of the PR diff against `origin/main`. Tell it to read `AGENTS.md` / `CLAUDE.md`, make no edits, and return numbered findings with severity, confidence, file:line, impact, and exact fix.
2. Spawn a second review-only Codex subagent using the silent-failure-hunter contract: find swallowed errors, broad/empty catch blocks, log-and-continue paths, hidden fallbacks, unawaited async work, retry exhaustion without user-visible failure, and missing log/user-feedback context. Tell it to make no edits and return numbered findings with severity, confidence, file:line, hidden failure, user/debugging impact, and exact fix.

No fallback: if the current runtime cannot run both internal reviewers in its branch, stop and report that internal review could not be completed; do not run a cross-runtime substitute, do not run an inline substitute, and do not proceed to step 4.

Hold findings; address them in step 4. After all required internal reviews complete for the current PR head SHA, write that SHA to `/tmp/.x-pr-reviewed-${PR_NUM}`.

Do not run internal review again during the same `/mando-pr` invocation, even if step 4 creates fix commits; the PR status/comment loop still runs normally.

## Step 4 — Address everything until merge-ready

Fix every internal-review finding from step 3, commit, push. Then use the wait-aware PR status gate:

```bash
python3 ~/.claude/skills/mando-pr/pr_status.py --watch <pr_number>
```

Address anything it reports, reply to review threads when appropriate, commit and push fixes, then rerun until `ALL CLEAR`. Do not wrap `pr_status.py` in manual shell delay or polling commands.

`--fast`: run the status check once, address what it surfaces in a single pass, stop.

Reply to threads via `gh api repos/{owner}/{repo}/pulls/{pr}/comments` with `in_reply_to={comment_id}` (top-level `gh pr comment` won't thread). The status script considers a thread "addressed" once the PR author has replied.

`git status` must be clean before finishing — commit or delete leftover screenshots, temp files, build artifacts.

## Notes

1. If on `main`, branch first.
2. Squash-merge convention.

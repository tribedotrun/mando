---
name: mando-pr
description: Commit, push, create PR, and tag AI reviewers. Use when ready to open a pull request — NOT for intermediate commits. Pass `--fast` for low-risk changes (skips internal review + monitoring loop).
---

## Arguments

- `--fast` — Skip internal review agents and monitoring loop. Single-pass comment addressing only. Use for low-risk changes you'd self-approve.

## Workflow

1. **Sync with main**: Run `git fetch origin main && git merge origin/main --no-edit` to merge latest main and resolve any conflicts before proceeding. If conflicts require user input, stop and report.

2. **Verify wiring and UI surfacing**: Review `git diff origin/main..HEAD` and confirm: (a) every new public function, route handler, component, config field, and CLI/TG command is called, registered, rendered, or read from a user-facing entry point; (b) every user-visible feature added to the daemon, API, captain, CLI, or Telegram has a corresponding Electron UI update (view, indicator, setting, notification, or SSE event). Fix any gaps before proceeding.

3. **Commit & push**: Stage changes, commit with a descriptive message, and `git push`

4. **Rename branch if needed** (MUST happen before PR creation — renaming after closes the PR):

   Rename if branch name is generic (e.g., `feat/1`, `feat/wt-0211-2120`, `codex/c29a-coverage-100`) or doesn't describe the changes. Read `git log main..HEAD --oneline` to understand changes, then pick prefix from dominant commit type (`feat/`, `fix/`, `chore/`, `refactor/`) + kebab-case summary (3-5 words max). Rename local, push new, delete old remote, set upstream.

   **If a PR already exists** for this branch (`gh pr view --json number 2>/dev/null` succeeds): do NOT rename. The rename procedure deletes the old remote ref, which causes GitHub to close the PR. The `x-task-log` skill handles branch renaming before PR creation to prevent this.

5. **Create PR** (skip if one already exists — e.g. draft from `x-task-log`; check with `gh pr view --json number 2>/dev/null`). Use body-file to avoid escaping issues:

   ```bash
   cat > /tmp/pr-body.md << 'EOF'
   ## Problem
   <What's broken, missing, or suboptimal — the motivation for this PR>

   > **Original request**: <Copy the original human prompt verbatim from the brief's "Original Request" section. If no such section exists, omit this quote block.>

   ## Testing & Verification

   ### Unit tests
   <New/modified unit tests and results>

   ### E2E regression
   <mando-dev test / mando-dev check results — confirms nothing broke>

   ### E2E verification
   <Bespoke steps exercising the exact new behavior against a running system (no mocks). Include commands run + observed output. If not yet performed, leave empty — mando-pr-summary will flag it.>
   EOF

   gh pr create --title "..." --body-file /tmp/pr-body.md
   rm /tmp/pr-body.md
   ```

6. **Convert draft to ready** (ALWAYS check and convert):

   ```bash
   gh pr view --json isDraft,number --jq 'select(.isDraft) | .number' | xargs -I {} gh pr ready {}
   ```

7. **Update PR summary**: Run `/mando-pr-summary`

8. **Trigger CI gate + external AI reviews** (idempotent — skip if already triggered):

   ```bash
   # Only post trigger comments if they don't already exist
   PR_NUM=$(gh pr view --json number -q .number)
   EXISTING=$(gh pr view "$PR_NUM" --json comments -q '.comments[].body' 2>/dev/null || true)

   echo "$EXISTING" | grep -qF "/ci" || gh pr comment -b "/ci"  # informational, not required to merge
   echo "$EXISTING" | grep -qF "@codex review" || gh pr comment -b "@codex review this PR"
   echo "$EXISTING" | grep -qF "cursor review" || gh pr comment -b "cursor review"
   ```

9. **Address comments and reviews**:

   **If `--fast`**: single pass, no loop, no internal review agents.

   Run the status check once:

   ```bash
   python3 ~/.claude/skills/mando-pr/pr_status.py <pr_number>
   ```

   - `UNADDRESSED COMMENTS` → fix ALL issues, reply to EACH comment, commit & push with `git add . && git commit -m "..." && git push`
   - `[FAIL]` CI checks → inspect, fix code, commit & push with `git add . && git commit -m "..." && git push`
   - `[WAIT]` or `ALL CLEAR` → proceed to step 10

   **If not `--fast`**: internal review + monitoring loop.

   a. **Run internal review** (idempotent — skip if HEAD already reviewed):

      ```bash
      PR_NUM=$(gh pr view --json number -q .number)
      HEAD=$(git rev-parse HEAD)
      REVIEW_FILE="/tmp/.x-pr-reviewed-${PR_NUM}"
      if [ -f "$REVIEW_FILE" ] && [ "$(cat "$REVIEW_FILE")" = "$HEAD" ]; then
        echo "Skipping internal review — HEAD $HEAD already reviewed"
      fi
      ```

      If not skipped (DO NOT run in background — wait for results):
      - Use `pr-review-toolkit:code-reviewer` agent to review the diff
      - Use `pr-review-toolkit:silent-failure-hunter` agent if error handling is involved

      After review completes, record the reviewed SHA:

      ```bash
      echo "$HEAD" > "$REVIEW_FILE"
      ```

   b. **Fix all internal issues**: Address problems, commit & push with `git add . && git commit -m "..." && git push`. Repeat review if significant changes were made.

   c. **Monitor PR status** (loop until all clear):

      ```bash
      python3 ~/.claude/skills/mando-pr/pr_status.py <pr_number>
      ```

      **Exit codes**: `0` = all clear, `1` = has issues (details in stdout).

      **Loop logic** (repeat until exit 0):
      - `[FAIL]` CI checks → inspect, fix code, commit & push with `git add . && git commit -m "..." && git push`, re-trigger CI (see below), re-check
      - `UNADDRESSED COMMENTS` → fix ALL issues, reply to EACH comment (see below), commit & push, re-trigger CI (see below), re-check
      - `[WAIT]` CI or reviewers pending → sleep 10s, re-check
      - `ALL CLEAR` → proceed to step 10

      **Re-trigger CI after pushing fixes** (required after every commit & push in this loop):

      ```bash
      gh pr comment $PR_NUM -b "/ci"
      ```

      Unlike step 8's idempotent guard, this ALWAYS posts a new `/ci` comment because the HEAD has changed and CI needs to run against the new code.

   d. **Consolidate** remaining issues into a table with Reviewer, Issue, and Location columns. Fix all in ONE commit with `git add . && git commit -m "..." && git push`.

   **Replying to review comments** (both modes — required for each unaddressed comment):

   ```bash
   gh api repos/{owner}/{repo}/pulls/{pr}/comments -X POST \
     -f body="Fixed in <commit_sha>: <brief explanation>" \
     -F in_reply_to={comment_id}
   ```

   The status script detects unaddressed threads — a comment is "addressed" when the PR author has replied in the thread.

10. **Verify clean git state** (MUST be last step):

    ```bash
    git status --short
    ```

    If there are unstaged changes, modified files, or untracked files (screenshots, temp files, build artifacts): commit them with `git add . && git commit -m "..." && git push` or delete them. A dirty worktree means forgotten work or leaked artifacts. Do NOT proceed until `git status` is clean.

## Notes

- If on `main` branch, create a feature branch first
- Use squash merge convention (one commit per feature)
- **`--fast` requires explicit opt-in**: Only use `--fast` mode when the user literally passes `--fast` as an argument (e.g. `/mando-pr --fast`). Never infer fast mode from context, risk level, or your own judgment. Default is always the full review + monitoring loop.

---
name: x-archive-plans
description: Batch archive plan folders from merged PRs. Copies to ~/.ai/plans/, removes from git, commits.
---

# Archive Plans

Batch archive `.ai/plans/` folders whose PRs are already merged.

## Instructions

### Step 1 — Find plan folders

```bash
ls -d .ai/plans/pr-*/ .ai/plans/ABR-*/ 2>/dev/null
```

If none exist, report "nothing to archive" and stop.

### Step 2 — Check which are safe to archive

For each `pr-*` folder, check if the PR is merged:

```bash
gh pr view <NUM> --json state -q '.state'
```

Only archive folders where state is `MERGED`. For `ABR-*` folders, check if there's a corresponding merged PR on the current repo — skip any that are still open or have no PR.

### Step 3 — Archive and remove

For each folder safe to archive:

```bash
REPO_NAME=$(basename $(git rev-parse --show-toplevel))
ARCHIVE_DIR="$HOME/.ai/plans/$REPO_NAME/$(basename $PLAN_DIR)"
mkdir -p "$ARCHIVE_DIR"
cp -r "$PLAN_DIR"/* "$ARCHIVE_DIR"/
git rm -r "$PLAN_DIR"
```

### Step 4 — Commit

Commit all removals in a single commit with `/x-git-commit`. Message: `chore: archive N plan folders from merged PRs`.

Report what was archived and what was skipped.

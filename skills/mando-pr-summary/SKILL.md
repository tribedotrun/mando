---
name: mando-pr-summary
description: Generate end-to-end PR summary diagram + reviewer checklist. Auto-creates a PR if none exists. Updates PR description and saves to plan folder.
---

## Instructions

### Step 1 — Get PR data (auto-create if missing)

Check if a PR exists for the current branch:

```bash
PR_NUM=$(gh pr view --json number -q '.number' 2>/dev/null)
```

If no PR exists (`$PR_NUM` is empty or the command fails), create one automatically as a prerequisite:

1. Commit any uncommitted changes (`git add . && git commit -m "..." && git push`).
2. Push the branch: `git push -u origin HEAD`
3. Create a minimal PR:
   ```bash
   gh pr create --title "<short title from branch name or recent commits>" --body ""
   ```
4. Re-read the PR number:
   ```bash
   PR_NUM=$(gh pr view --json number -q '.number')
   ```

> **Note**: This uses a minimal `gh pr create` (not the full `/mando-pr` flow) to avoid circular invocation, since `mando-pr` step 7 calls `/mando-pr-summary`.

Then fetch PR data:

```bash
gh pr view $PR_NUM --json title,body,files,commits,headRefName,baseRefName
gh pr diff $PR_NUM
```

### Step 2 — Analyze the diff

Read the full diff. Identify:

- **Trigger**: what user action or system event starts the flow
- **Data path**: end-to-end from trigger → API/service → processing → response → UI
- **Parallel steps**: any `Promise.all`, `Promise.allSettled`, concurrent fetches
- **Key data transformations**: filtering, enrichment, aggregation, caching
- **Response shape**: what the caller receives

### Step 3 — Generate the diagram

Generate the diagram inline following these conventions:
- Use `┌─┐│└─┘` box-drawing characters for boxes
- Use `▼` for vertical flow, `──→` for horizontal, `←` for responses
- All boxes within a container share the same width (pad with spaces)
- Keep lines under 80 chars where possible, max 100
- Show: component names, responsibilities, data shapes at boundaries
- Don't show: internal helpers, implementation details

Enhance with PR-specific details:
- Stage names, data shapes, cache strategies, parallel boundaries, external service calls
- Specific details that matter for review (TTLs, thresholds, graceful degradation)
- Key architectural names — stores, hooks, services, route paths, repo modules
- **Cache patterns**: Show as decision flow: `cache (TTL) → fallback on miss`
- **Response shape**: Show the actual JSON structure the caller receives

### Step 4 — Generate the reviewer checklist

```markdown
### Reviewer Checklist

- [ ] **DB migration**: <describe columns/tables, or "none">
- [ ] **Env vars**: <list new vars, or "none">
- [ ] **New dependencies**: <list packages, or "none">
- [ ] **Mobile**: <"JS-only, no new dev build needed" or "needs new dev build because X">
- [ ] **Backend deploy**: <which services need deploy, or "no backend changes">
- [ ] **Breaking changes**: <describe, or "none">
- [ ] **External API calls**: <service + rate limit/cache info, or "none added">
- [ ] **No backward-compat / legacy code**: <confirm no shims, deprecated re-exports, legacy fallbacks, or flag violations>
- [ ] **Wiring**: <for each new function, route, component, config field, CLI/TG command — confirm it is called/registered/rendered/read from a user-facing entry point; list any gaps>
- [ ] **Electron UI surfacing**: <for each user-visible feature added to daemon/API/captain/CLI/TG — confirm it is reflected in the Electron app (view, indicator, setting, notification, or SSE event); list any gaps>
- [ ] **Wiki updated**: <if routes changed → `docs/wiki/api-reference.md` updated; if architecture/captain/integrations changed → `docs/wiki/architecture.md` updated; or "no wiki-relevant changes">
```

### Step 5 — Fix diagram alignment

```bash
echo '<diagram>' | python3 ~/.claude/skills/mando-pr-summary/fix-diagram.py
```

Use the script's output as the final diagram.

### Step 6 — Output the summary in the conversation

**Before** writing to files or updating the PR, output the full summary directly so the user sees it in Claude Code:

1. The ASCII diagram in a fenced code block (after alignment)
2. The "What changed" sentence
3. The reviewer checklist
4. Whether e2e verification is missing (flag it)

### Step 7 — Evidence (screenshots/recordings)

Resolve the plan folder (same as x-task-log):
1. `git branch --show-current | grep -oE 'ABR-[0-9]+'` → `.ai/plans/ABR-{id}/`
2. Fallback: `.ai/plans/pr-$PR_NUM/`

Then inspect both:
- the current PR body (to see whether `## Evidence` already contains hosted evidence or runtime output)
- the resolved plan folder for `before-*.{png,mp4}` or `evidence-*.{png,mp4}`

Apply this hosting decision:
- **Bucket present**: keep the current GCS flow and generate a fresh hosted Evidence section from the local files.
- **Bucket missing**:
  - preserve existing GitHub attachment-backed evidence and substantive runtime output
  - if new local visual files exist but no hosted attachment URL exists yet, leave this note in `## Evidence` and list the pending filenames instead of silently dropping the visuals:

```markdown
> **Action required**
> Local PR evidence is ready, but this checkout has no `MANDO_DEV_GCS_BUCKET`.
> Upload the pending file(s) to the PR description with GitHub’s web editor, then rerun `/mando-pr-summary`.
> Pending files: `<file1>`, `<file2>`
```

  - never substitute raw branch/blob URLs
- **No local visuals and no existing substantive evidence**: omit `## Evidence`.

### Step 8 — Flag missing e2e verification

Scan the existing PR body for the `### E2E verification` subsection. If it is empty or contains only placeholder text, insert a warning:

```markdown
> **Warning**
> E2E verification is missing. This PR has no bespoke proof that the new behavior works against a running system.
```

### Step 9 — Compose and update PR description

Get the HEAD SHA for the freshness marker:

```bash
SHORT_SHA=$(git rev-parse --short HEAD)
```

Combine into a single markdown body. **Preserve all existing PR content that is NOT part of `## PR Summary` / `### Reviewer Checklist` sections** — this includes `## Problem`, `## Testing & Verification`, and third-party integration blocks (e.g., "Open in Devin", review badges, deploy previews).

For `## Evidence`, follow the Step 7 hosting decision exactly:
- replace it only when you have fresh hosted evidence for the current code state
- preserve existing substantive hosted evidence when no bucket is configured
- if local visuals are pending manual GitHub upload, write the Step 7 **Action required** note instead of deleting the section

Format:

```markdown
<existing PR description — Problem, Testing & Verification, etc.>

---

## PR Summary

\```
<ASCII diagram>
\```

<!-- pr-summary-head: <SHORT_SHA> -->

**What changed**: <1-2 sentence high-level delta — what was the old behavior vs new>

## Evidence

<GCS-hosted visuals, preserved GitHub attachment visuals, runtime output, or the pending-upload note — or omit if none>

### Reviewer Checklist

<checklist items>
```

Update with HEREDOC to avoid escaping:

```bash
gh pr edit $PR_NUM --body "$(cat <<'PRBODY'
<full composed body here>
PRBODY
)"
```

### Step 10 — Save to plan folder

Resolve the plan folder (same priority as Step 7). Write the same summary to `.ai/plans/<resolved>/pr-summary.md`. Create folder if needed. Overwrite if exists (always regenerated from current diff).

**Important**: Never write into a plan folder that doesn't belong to the current PR. If no matching folder exists, create `.ai/plans/pr-$PR_NUM/`.

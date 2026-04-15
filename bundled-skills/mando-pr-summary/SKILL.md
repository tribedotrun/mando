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

Start with the universal checklist. Then check if the repo's `CLAUDE.md` has a `## PR Checklist` section -- if so, append those items.

```markdown
## Reviewer Checklist

- [ ] **DB migration**: <describe columns/tables, or "none">
- [ ] **Env vars**: <list new vars, or "none">
- [ ] **New dependencies**: <list packages, or "none">
- [ ] **Backend deploy**: <which services need deploy, or "no backend changes">
- [ ] **Breaking changes**: <describe, or "none">
- [ ] **External API calls**: <service + rate limit/cache info, or "none added">
- [ ] **No backward-compat / legacy code**: <confirm no shims, deprecated re-exports, or legacy fallbacks>
- [ ] **Wiring**: <for each new function, route, component, config field, or command -- confirm it is called/registered/rendered/read from a user-facing entry point; list any gaps>
<if CLAUDE.md has ## PR Checklist, append each item here>
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

Check for local evidence files saved via `mando todo evidence` (stored as task artifacts) or in the plan folder (`before-*.{png,mp4}`, `evidence-*.{png,mp4}`).

- **No evidence files**: omit `## Evidence`.
- **Evidence files exist**: upload and embed URLs in `## Evidence`.
  1. If `MANDO_DEV_GCS_BUCKET` is set: upload to GCS (`gcloud storage cp <file> gs://$MANDO_DEV_GCS_BUCKET/pr-$PR_NUM/<filename>`), reference via `https://storage.googleapis.com/$MANDO_DEV_GCS_BUCKET/pr-$PR_NUM/<filename>`.
  2. Otherwise: upload to GitHub as release assets. Create a prerelease tagged `pr-$PR_NUM-evidence`, upload files, and embed the download URLs:
     ```bash
     gh release create "pr-$PR_NUM-evidence" <file1> <file2> \
       --prerelease --title "PR #$PR_NUM evidence" --notes ""
     # Get URL for each file:
     gh api repos/{owner}/{repo}/releases/tags/pr-$PR_NUM-evidence \
       --jq '.assets[] | select(.name=="<filename>") | .browser_download_url'
     ```
     If the release already exists, upload additional assets with `gh release upload`.

### Step 8 — Flag missing e2e verification

Scan the existing PR body for the `### E2E verification` subsection. If it is empty or contains only placeholder text, insert a warning:

```markdown
> **Warning**
> E2E verification is missing. This PR has no bespoke proof that the new behavior works against a running system.
```

### Step 9 — Compose and update PR description

Always produce the full canonical PR structure below. This skill owns the entire PR body -- generate every section fresh from the diff, brief, and evidence.

For `## Evidence`, follow the Step 7 hosting decision exactly:
- evidence uploaded (GCS or GitHub release) → embed the URLs
- no evidence files → omit the section

Canonical structure:

```markdown
## Problem

<what's broken, missing, or suboptimal -- the motivation for this PR. Include the original request verbatim if available from the brief.>

## Solution

\```
<ASCII diagram>
\```

**What changed**: <1-2 sentence high-level delta -- what was the old behavior vs new>

## Evidence

<per Step 7 hosting decision -- or omit if none>

## Reviewer Checklist

<enriched checklist from Step 4>

## Testing & Verification

<Carry forward substantive content from the existing PR body for this
section. Only generate fresh if empty or placeholder.>

### Unit tests
### E2E regression
### E2E verification
```

Preserve third-party integration blocks (e.g., "Open in Devin", review badges, deploy previews) by appending them after the canonical sections.

Update with HEREDOC to avoid escaping:

```bash
gh pr edit $PR_NUM --body "$(cat <<'PRBODY'
<full composed body here>
PRBODY
)"
```

### Step 10 — Save work summary to DB and plan folder

**Save to DB** (required): Write the ASCII diagram + "What changed" sentence to a temp file, then call the CLI to persist it as a work summary artifact:

```bash
cat > /tmp/work-summary.md << 'SUMMARY'
```
<ASCII diagram>
```

**What changed**: <1-2 sentence delta>
SUMMARY
mando todo summary --file /tmp/work-summary.md
rm /tmp/work-summary.md
```

**Save to plan folder** (secondary): Resolve the plan folder (same priority as Step 7). Write the same summary to `.ai/plans/<resolved>/pr-summary.md`. Create folder if needed. Overwrite if exists (always regenerated from current diff).

**Important**: Never write into a plan folder that doesn't belong to the current PR. If no matching folder exists, create `.ai/plans/pr-$PR_NUM/`.

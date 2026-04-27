---
name: mando-pr-summary
description: Generate end-to-end PR summary diagram + reviewer checklist. Auto-creates a PR if none exists. Updates PR description and saves to plan folder. Built for a normal coding-agent session; Mando-task extras apply only when MANDO_TASK_ID is set.
---

## Session context

**Default — regular coding session:** No Mando task in play. Run the steps as written; ignore every **Mando task only** subsection.

**Mando task session:** `MANDO_TASK_ID` is set. Do everything the default path does **and** follow each **Mando task only** subsection.

---

## Step 1 — Get PR data

Resolve the current branch's PR number. If none exists, create a minimal PR with an empty body (commit and push first).

## Step 2 — Analyze and diagram

Read the full diff. Identify the trigger, end-to-end data path (trigger → API/service → processing → response → UI), parallel steps, key transformations, and the response shape. Capture a 1–2 sentence "What changed" delta.

Generate an ASCII diagram using `┌─┐│└─┘` for boxes, `▼ ──→ ←` for flow. Show component names, responsibilities, data shapes at boundaries; hide internal helpers. Surface PR-specific details that matter for review when the diff actually involves them — caching, parallel boundaries, external calls, response shape, thresholds, key architectural names.

Align via `python3 ~/.claude/skills/mando-pr-summary/fix-diagram.py` (pipe in, use the output).

## Step 3 — Build the reviewer checklist

Universal items (no heading — Step 5's body supplies it). Append items from the repo's `## PR Checklist` if present.

```markdown
1. [ ] **DB migration**: <columns/tables, or "none">
2. [ ] **Env vars**: <new vars, or "none">
3. [ ] **New dependencies**: <packages, or "none">
4. [ ] **Backend deploy**: <which services, or "no backend changes">
5. [ ] **Breaking changes**: <describe, or "none">
6. [ ] **External API calls**: <service + rate limit/cache, or "none added">
7. [ ] **No backward-compat / legacy code**: <confirm no shims, deprecated re-exports, or legacy fallbacks>
8. [ ] **Wiring**: <every new function, route, component, config field, or command is called/registered/rendered/read from a user-facing entry point; list any gaps>
```

## Step 4 — Handle evidence

**Default:** Find local before/after or proof media the session or human saved (images, recordings).

**Mando task only:** Also include artifacts attached via `mando todo evidence`.

1. No qualifying files → say so in `## Evidence`.
2. Files exist → host and embed:
   1. `MANDO_DEV_GCS_BUCKET` set → upload to `gs://$MANDO_DEV_GCS_BUCKET/pr-$PR_NUM/<filename>`; reference at `https://storage.googleapis.com/$MANDO_DEV_GCS_BUCKET/pr-$PR_NUM/<filename>`.
   2. Otherwise → attach to a GitHub prerelease tagged `pr-$PR_NUM-evidence` (create if it doesn't exist), and link the published download URLs.

## Step 5 — Preview, compose, persist

**Preview first.** Output the aligned diagram (fenced), the "What changed" sentence, the reviewer checklist, and any e2e-verification gap so the user sees it in conversation.

**Compose the canonical PR body.** This skill owns the entire body — regenerate every section fresh from diff + brief + evidence.

````markdown
## Problem

<what's broken/missing/suboptimal — the motivation. Include the original request verbatim if available from the brief.>

## Solution

\```
<ASCII diagram>
\```

**What changed**: <1–2 sentence delta>

## Evidence

<per Step 4>

## Reviewer Checklist

<universal + project-specific items from Step 3>

## Testing & Verification

### Unit tests

<what ran or should run; scope of change>

### E2E regression

<implied suites, or "not run" / N/A with reason>

### E2E verification

<per-PR verify plan path or concrete proof; or what would suffice>
````

If **E2E verification** has no concrete proof (no plan path, no run noted, no artifact), prepend:

```markdown
> **Warning**
> E2E verification is missing. This PR has no bespoke proof that the new behavior works against a running system.
```

Preserve third-party blocks (Open in Devin, review badges, deploy previews) by appending after the canonical sections. Update via `gh pr edit $PR_NUM --body` using a HEREDOC.

**Persist the work summary** (ASCII diagram + "What changed" sentence):

**Default:** Write to `.ai/plans/pr-$PR_NUM/pr-summary.md` (overwrite). Create the folder if missing. Never write into another `.ai/plans/*` folder, even if a slug looks related.

**Mando task only:** After the plan file, write the same summary to a temp file and run `mando todo summary --file <path>`. Never infer a task id from PR number, branch, or plan folder — only the env var qualifies.

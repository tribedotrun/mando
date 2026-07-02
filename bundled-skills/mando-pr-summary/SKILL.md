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

Treat the code grouping as the key feature of the summary. Group the changed code into a few logical Code Diff sections, usually 3–6. Use reviewer-facing domains such as "API route", "runtime ingest", "DB schema", "UI", "SDK", "tests", or PR-specific names. Each section must explain: what changed, why it matters, how to review it, and representative files or surfaces. Do not list files one-by-one when a logical group communicates the review path better.

Generate an ASCII diagram using `┌─┐│└─┘` for boxes, `▼ ──→ ←` for flow. Show component names, responsibilities, data shapes at boundaries; hide internal helpers. Surface PR-specific details that matter for review when the diff actually involves them — caching, parallel boundaries, external calls, response shape, thresholds, key architectural names.

Align via `python3 ~/.claude/skills/mando-pr-summary/fix-diagram.py` (pipe in, use the output).

## Step 3 — Build the reviewer checklist

Universal items (no heading — Step 5's body supplies it). Append items from the repo's `## PR Checklist` if present.

For **Env vars**, report the `/mando-pr` audit. Do not edit code here. If the audit is missing or an env lacks rationale, stop and return to `/mando-pr` Step 2. Checklist says `none` or `<VAR>: <why env boundary>`.

```markdown
1. [ ] **DB migration**: <columns/tables, or "none">
2. [ ] **Env vars**: <none, or each new env var with why it needs env; unnecessary envs removed>
3. [ ] **New dependencies**: <packages, or "none">
4. [ ] **Backend deploy**: <which services, or "no backend changes">
5. [ ] **Breaking changes**: <describe, or "none">
6. [ ] **External API calls**: <service + rate limit/cache, or "none added">
7. [ ] **No backward-compat / legacy code**: <confirm no shims, deprecated re-exports, or legacy fallbacks>
8. [ ] **Wiring**: <every new function, route, component, config field, or command is called/registered/rendered/read from a user-facing entry point; list any gaps>
9. [ ] **UI copy**: <all new/edited user-facing UI strings surfaced for human review, or "none">
10. [ ] **Architecture surface**: <domains/layers touched per repo architecture guidance, or "N/A">
```

## Step 4 — Handle evidence

**Default:** Find local proof media the session or human saved. UI changes ship three artifacts (a before screenshot, an after screenshot, and an after recording); non-UI changes ship terminal output.

**Mando task only:** Also include artifacts attached via `mando todo evidence`.

1. No qualifying files → say so in `## Evidence`.
2. Files exist → host and embed:
   1. `MANDO_DEV_GCS_BUCKET` set → upload to `gs://$MANDO_DEV_GCS_BUCKET/pr-$PR_NUM/<filename>`; reference at `https://storage.googleapis.com/$MANDO_DEV_GCS_BUCKET/pr-$PR_NUM/<filename>`.
   2. Otherwise → attach to a GitHub prerelease tagged `pr-$PR_NUM-evidence` (create if it doesn't exist), and link the published download URLs.
3. Embed each artifact — plain `[label](url)` does not render inline:
   1. `.png` / `.jpg` / `.gif` / `.webp` → `![<caption>](<url>)` on its own line.
   2. `.webm` / `.mp4` / `.mov` → convert with `ffmpeg -i in.webm -vf "fps=12,scale=720:-1:flags=lanczos" -loop 0 out.gif`, upload both, embed the GIF and link the original. GitHub strips `<video src=ext>`.
   3. JSON / text logs → `[<filename>](<url>)`.

## Step 5 — Preview, compose, validate, persist

**Preview first.** Output the Code Diff groups first, then the aligned diagram (fenced), the "What changed" sentence, the reviewer checklist, and any e2e-verification gap so the user sees it in conversation.

**Compose the canonical PR body.** This skill owns the entire body — regenerate every section fresh from diff + brief + evidence.

````markdown
## Problem

<what's broken/missing/suboptimal — the motivation. Include the original request verbatim if available from the brief.>

## Solution

\```
<ASCII diagram>
\```

**What changed**: <1–2 sentence delta>

## Code Diff

<3–6 logical code groups. Each group: what changed, why it matters, how to review it, and representative files/surfaces. This is the main reviewer navigation feature.>

## Evidence

<per Step 4. UI shape:>

![before-<name>](<png url>)
![after-<name>](<png url>)
![after-<name>](<gif url>)

Full-quality video: [<name>.webm](<webm url>)

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

If **E2E verification** has no concrete proof (no plan path, no run noted, no artifact), put `E2E verification missing.` in that section.

Preserve third-party blocks (Open in Devin, review badges, deploy previews) by appending after the canonical sections. Update the PR from the composed canonical body only, using a temporary full-body file and `gh pr edit --body-file "$BODY_FILE"`. Never pass `.ai/plans/pr-$PR_NUM/pr-summary.md` to `gh pr edit`; that file is not the PR body.

**Shell safety for Markdown bodies.** PR bodies contain backticks, `$VARS`, `@skill` mentions, glob-looking route names, and checklist brackets. Do not write them with an unquoted heredoc such as `cat <<EOF`, and do not inline them through `gh pr edit --body "..."`; zsh/bash will execute command substitutions, expand globs, and may accidentally run commands named in code spans. Use one of these safe patterns:

```bash
cat > "$BODY_FILE" <<'EOF'
<full markdown body with literal backticks, $VARS, [brackets], and @mentions>
EOF

gh pr edit "$PR_NUM" --body-file "$BODY_FILE"
```

If you need to insert generated text such as an ASCII diagram, write the body with Python or another quoted-template mechanism that treats Markdown as data, then validate the final file before calling `gh pr edit`. If a body-generation command starts emitting `command not found`, `bad pattern`, or `no matches found`, interrupt it, discard the partial body, and regenerate with a quoted template before touching the PR.

**Validate the live PR body.** Immediately after `gh pr edit`, fetch and validate the GitHub body:

```bash
python3 ~/.claude/skills/mando-pr-summary/validate-pr-body.py --pr "$PR_NUM"
```

The validation must pass before this skill is considered complete. If it fails, regenerate the full canonical PR body and update GitHub again. Do not continue with a body that is missing canonical sections or that looks like the persisted work-summary-only artifact.

**Persist the work-summary-only artifact** (Code Diff groups + ASCII diagram + "What changed" sentence):

**Default:** Write to `.ai/plans/pr-$PR_NUM/pr-summary.md` (overwrite). Create the folder if missing. Never write into another `.ai/plans/*` folder, even if a slug looks related.

This persisted artifact is for local/Mando work summaries only. It intentionally omits Problem, Evidence, Reviewer Checklist, and Testing & Verification.

**Mando task only:** After the plan file, write the same summary to a temp file and run `mando todo summary --file <path>`. Never infer a task id from PR number, branch, or plan folder — only the env var qualifies.

**Optional HTML summary:** When the human asks for an HTML PR summary, copy `templates/pr-summary.html` to `.ai/plans/pr-$PR_NUM/pr-summary.html` and replace the `window.PR_SUMMARY` object at the top. This HTML is only for humans to understand the PR faster; it is not the authoritative PR description, not a required reviewer artifact, and not a second source of truth. Keep it disposable and PR-scoped. It should mirror the canonical PR body sections and make the Code Diff groups the primary tab so reviewers can navigate the change logically before reading Problem/Solution or checklist details.

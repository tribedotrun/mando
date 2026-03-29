---
name: mando-linear-workpad
description: Create and maintain a persistent workpad comment on a Linear issue. Use to track progress, blockers, and PR links.
---

# Linear Workpad

Maintain a single persistent `## Mando Workpad` comment on a Linear issue that tracks agent progress.

## Setup

```bash
source ~/.secrets  # LINEAR_API_KEY
```

The `linear` CLI is at `$(readlink -f ~/.claude/skills/mando-linear)/linear`.

## Instructions

### Step 1 — Find or create workpad comment

Check if a workpad comment already exists:

```bash
linear comment-list <ISSUE-ID>
```

Look for a comment whose body starts with `## Mando Workpad`. If found, note the `comment_id`.

If not found, create one:

```bash
linear comment <ISSUE-ID> "## Mando Workpad

\`\`\`text
$(hostname):$(pwd)@$(git rev-parse --short HEAD)
\`\`\`

### Plan
- [ ] (fill in your implementation plan)

### Status
In progress

### Blockers
None

### PR
(none yet)
"
```

Then run `linear comment-list <ISSUE-ID>` to get the comment_id of the one you just created.

### Step 2 — Update workpad as you progress

After each milestone (plan finalized, PR created, tests passing, blocker hit):

```bash
linear comment-update <COMMENT-ID> "## Mando Workpad

\`\`\`text
$(hostname):$(pwd)@$(git rev-parse --short HEAD)
\`\`\`

### Plan
- [x] Step 1 completed
- [x] Step 2 completed
- [ ] Step 3 in progress

### Status
Implementation in progress — tests passing

### Blockers
None

### PR
#<PR_NUM>
"
```

### Step 3 — Record blockers

If you hit a true blocker:

```bash
linear comment-update <COMMENT-ID> "## Mando Workpad
...
### Blockers
- Missing API key for service X — need human to provide
...
"
```

## Workpad Template

```markdown
## Mando Workpad

\`\`\`text
<hostname>:<abs-workdir>@<short-sha>
\`\`\`

### Plan
- [ ] Step 1
- [ ] Step 2

### Status
<current status>

### Blockers
<None or list of blockers>

### PR
<#number or "none yet">
```

## Rules

- Never create duplicate workpad comments — always find and update the existing one
- Keep the env stamp updated (re-run hostname/pwd/sha on each update)
- Do not include metadata already visible in Linear (issue ID, title, state)
- Update on every significant milestone, not on every commit

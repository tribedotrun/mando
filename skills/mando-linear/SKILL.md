---
name: mando-linear
description: Linear CLI for issue management. Query, create, update, and manage Linear issues directly.
---

# Linear CLI

Manage Linear issues without leaving the terminal. The CLI script is bundled at `linear` in this skill directory.

## Setup

Requires `LINEAR_API_KEY` env var (`lin_api_...` token). Source from `~/.secrets`.

```bash
source ~/.secrets
```

## CLI Reference

```
linear get <ABR-123>                  # Get issue details
linear get-by-id <uuid>               # Get issue by UUID
linear search "query"                 # Search issues (title + description)
linear my                             # List my open assigned issues
linear create <team> "title" [opts]   # Create issue
linear update <ABR-123> "field=val"   # Update issue (title, priority)
linear update <ABR-123> description   # Update description (pipe to stdin)
linear status <ABR-123> "State"       # Move to workflow state
linear comment <ABR-123> "text"       # Add comment (returns id)
linear comment-list <ABR-123>         # List comments (id + preview)
linear comment-update <uuid> "text"   # Update comment by ID
linear label <ABR-123> "label"        # Add label (additive)
linear unlabel <ABR-123> "label"      # Remove label
linear delete <ABR-123>               # Delete issue
linear states <team>                  # List workflow states
linear teams                          # List teams
```

### Create flags

- `-d "desc"` — Description
- `-l "label"` — Label (repeatable)
- `-p 1-4` — Priority: 1=urgent, 2=high, 3=medium (default), 4=low
- `--parent ABR-X` — Parent issue (for sub-issues)

## Usage

The CLI path is: `$(dirname "$0")/linear` (relative to this SKILL.md).

Always `source ~/.secrets` before invoking to ensure `LINEAR_API_KEY` is set.

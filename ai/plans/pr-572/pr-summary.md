# PR #572 — fix: transcript tool detection and input bar UX

## Diagram

```
┌──────────────────────────────────────────────────┐
│            TranscriptViewer.tsx                  │
│         (Electron session panel)                 │
├──────────────────────────────────────────────────┤
│  JSONL markdown (from Rust render_tool_use)      │
│                  ▼                               │
│  parseTurnBody() scans each line                 │
│                  ▼                               │
│  ┌────────────────────────────────────────────┐  │
│  │  Match: /^\*\*([\w.-]+)\*\*(.*)$/          │  │
│  │                  ▼                         │  │
│  │  Check 1 (format):  two-space convention   │──│─→ TOOL
│  │  Check 2 (known):   CC_TOOLS + mcp_        │──│─→ TOOL
│  │  Check 3 (fence):   next line is ```       │──│─→ TOOL
│  │  else → TEXT                               │  │
│  └────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────┐
│              TaskActionBar.tsx                   │
├──────────────────────────────────────────────────┤
│  Hidden: + captain-reviewing (NEW)               │
│  Textarea: auto-grow to 120px, reset on submit   │
│  Container: items-end (button anchored bottom)   │
└──────────────────────────────────────────────────┘
```

## What changed

Transcript viewer previously misidentified assistant bold text (like `**CODE**`, `**CI**`, `**EVIDENCE**`) as tool call blocks; now uses a two-layer detection (format convention + known CC tool names) to reliably distinguish real tool calls from bold markdown. Task input bar now auto-grows for multi-line input and hides during captain-reviewing.

## Reviewer Checklist

- [ ] **DB migration**: none
- [ ] **Env vars**: none
- [ ] **New dependencies**: none
- [ ] **Mobile**: N/A (Electron desktop only)
- [ ] **Backend deploy**: no backend changes
- [ ] **Breaking changes**: none
- [ ] **External API calls**: none added
- [ ] **No backward-compat / legacy code**: clean, no shims
- [ ] **Wiring**: both changes are in existing renderer components, no new routes/commands
- [ ] **Electron UI surfacing**: changes ARE the Electron UI
- [ ] **Wiki updated**: no wiki-relevant changes

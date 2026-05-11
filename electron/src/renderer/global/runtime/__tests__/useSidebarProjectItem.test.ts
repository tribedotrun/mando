// Regression for #133: clicking a project header in the sidebar did not
// collapse the project while the user was on a workbench page belonging
// to that project. It only collapsed after the user navigated away.
//
// Root cause: useSidebarProjectItem.ts re-asserted `setExpanded(true)` in
// the render body whenever `hasActiveWt` was true. The toggle path was:
//   1. user clicks header -> setExpanded(false)
//   2. component re-renders, hasActiveWt is still true, render-body
//      `if (hasActiveWt && !expanded) setExpanded(true)` snaps it back
//      open immediately.
// The fix gates the auto-expand on the active-cwd transition: only
// re-expand when `state.activeTerminalCwd` actually changes into a
// workbench in this project, not on every render that finds it still in.
//
// The renderer has no JSX test harness (no vitest, no testing-library —
// the repo uses `node --test`), so this file fences the source contracts
// that, taken together, prevent the bug from coming back.

import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const __dirname = dirname(fileURLToPath(import.meta.url));
const hookSrc = readFileSync(join(__dirname, '..', 'useSidebarProjectItem.ts'), 'utf8');

describe('useSidebarProjectItem auto-expand transition (regression #133)', () => {
  it('does NOT re-expand whenever hasActiveWt is true', () => {
    // The pre-fix `if (hasActiveWt && !expanded) setExpanded(true);` ran on
    // every render, snapping a manual collapse back open as long as the
    // active workbench was still inside this project.
    assert.doesNotMatch(
      hookSrc,
      /if\s*\(\s*hasActiveWt\s*&&\s*!expanded\s*\)\s*setExpanded\(\s*true\s*\)/,
      'useSidebarProjectItem must not unconditionally re-expand while ' +
        'hasActiveWt is true — that snaps a manual collapse back open ' +
        '(regression #133).',
    );
  });

  it('tracks state.activeTerminalCwd in a ref to detect the transition', () => {
    // The fix keys on the active-cwd transition so a manual collapse
    // sticks while the same active workbench remains, but a switch into
    // a different workbench in this project still legitimately re-expands.
    assert.match(
      hookSrc,
      /useRef\(\s*state\.activeTerminalCwd\s*\)/,
      'must store the previous state.activeTerminalCwd in a ref so the ' +
        'auto-expand fires on the transition, not on every render',
    );
    assert.match(
      hookSrc,
      /state\.activeTerminalCwd\s*!==\s*\w+\.current/,
      'transition detection must compare state.activeTerminalCwd against ' +
        'the previous-value ref',
    );
  });

  it('re-expand is gated on hasActiveWt so leaving the project does not auto-expand', () => {
    // Without the hasActiveWt gate, navigating *out* of this project (the
    // active cwd transitions to something not in this project) would still
    // satisfy the cwd-changed condition and force-expand a project the
    // user just collapsed.
    assert.match(
      hookSrc,
      /state\.activeTerminalCwd\s*!==\s*\w+\.current[\s\S]{0,80}hasActiveWt/,
      'auto-expand must require both an activeTerminalCwd transition AND ' +
        'hasActiveWt — otherwise leaving the project would re-expand it',
    );
  });
});

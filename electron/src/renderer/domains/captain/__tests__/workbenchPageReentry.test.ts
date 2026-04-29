// Regression for #120: creating a workbench could leave the user stuck on
// a centered "Preparing workspace..." text indefinitely (no spinner, no
// Cancel, no recovery).
//
// Root cause: `useWorkbenchPage` used `useMountEffect` (deps `[]`) to fire
// `openNewTerminal`, but TanStack Router reuses the WorkbenchPage component
// when only `params.workbenchId` (or only `search.project`) changes between
// values of the same route. So clicking the SquarePen "+ New terminal" icon
// while already on `/wb/<existingId>` updated the URL to `/wb/new?project=X`
// without remounting; the mount-effect never re-fired, `openNewTerminal`
// was never called, and `WorkbenchPage`'s dead fallback at lines 25-29
// painted a silent "Preparing workspace..." text — same shape if the user
// then clicked "+ New terminal" for a different project mid-creation.
//
// The renderer has no JSX test harness (no vitest, no testing-library —
// the repo uses `node --test`), so this file fences the source contracts
// that, taken together, prevent the bug from coming back:
//
// 1. `useWorkbenchPage` does NOT use `useMountEffect` for the
//    new-workbench creation branch (mount-effect deps `[]` is the bug).
// 2. `useWorkbenchPage` mirrors `useWorkbenchNav.ts:43-47`'s prevWbRef
//    pattern AND tracks `search.project` so a project switch on /wb/new
//    re-fires creation as well.
// 3. `useWorkbenchPage` navigates back to '/' when `openNewTerminal`
//    rejects so the user is never stranded on /wb/new with no recovery.
// 4. `openNewTerminal` accepts an `onError` callback and invokes it in
//    its catch block.
// 5. `cancelPreparing` resets `creatingRef` so a superseding
//    `openNewTerminal` (e.g. switching project mid-creation) is not
//    silently blocked behind the in-flight previous call.
// 6. `WorkbenchPage.tsx` no longer carries the silent dead "Preparing
//    workspace..." text fallback that was the user-visible end of the bug.

import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const __dirname = dirname(fileURLToPath(import.meta.url));

function readSrc(...parts: string[]): string {
  return readFileSync(join(__dirname, '..', ...parts), 'utf8');
}

const useWorkbenchPageSrc = readSrc('runtime', 'useWorkbenchPage.ts');
const useFeedbackWorktreeTerminalSrc = readSrc(
  'terminal',
  'runtime',
  'useFeedbackWorktreeTerminal.ts',
);
const workbenchPageSrc = readFileSync(
  join(__dirname, '..', '..', '..', 'app', 'routes', 'WorkbenchPage.tsx'),
  'utf8',
);

describe('useWorkbenchPage param-aware re-entry (regression #120)', () => {
  it('does NOT use useMountEffect to fire openNewTerminal', () => {
    // useMountEffect (deps `[]`) only runs once on mount. TanStack Router
    // reuses the component for `params.workbenchId` transitions of the
    // same route, so the mount-effect never re-fired and the user was
    // stranded on the dead fallback when navigating /wb/<id> -> /wb/new.
    assert.doesNotMatch(
      useWorkbenchPageSrc,
      /useMountEffect\s*\(/,
      'useWorkbenchPage must not use useMountEffect for the new-workbench ' +
        'creation branch — TanStack Router reuses the component on workbenchId ' +
        'transitions and the mount-effect would never re-fire (regression #120).',
    );
  });

  it('tracks workbenchId AND search.project with refs (handles both repro paths)', () => {
    // Two repro paths in the bug:
    //  (a) /wb/<existingId> -> /wb/new?project=X   (workbenchId changed)
    //  (b) /wb/new?project=A -> /wb/new?project=B  (only project changed)
    // Path (b) is the deferred edge from the original PR-1014 brief
    // (todo-106-106). Both must reset the creation guard and re-fire
    // openNewTerminal, so we fence the dual-ref pattern at source level.
    assert.match(
      useWorkbenchPageSrc,
      /prevWbRef\s*=\s*useRef\(\s*workbenchId/,
      "must mirror useWorkbenchNav.ts:43-47's prevWbRef pattern",
    );
    assert.match(
      useWorkbenchPageSrc,
      /prevProjectRef\s*=\s*useRef\(\s*search\.project/,
      'must also track search.project so a project switch on /wb/new re-fires creation',
    );
    assert.match(
      useWorkbenchPageSrc,
      /prevWbRef\.current\s*!==\s*workbenchId\s*\|\|\s*prevProjectRef\.current\s*!==\s*search\.project/,
      'transition detection must consider both workbenchId AND search.project',
    );
  });

  it('navigates home when openNewTerminal rejects', () => {
    // The pre-fix catch block toasted and set terminalPage = null but left
    // the URL on /wb/new — toast can be dismissed/missed and the dead
    // fallback would paint silently. Wire an onError callback that
    // navigate('/')-replaces so failure has a recovery surface.
    assert.match(
      useWorkbenchPageSrc,
      /openNewTerminal\([\s\S]*?navigate\(\s*\{\s*to:\s*['"]\/['"][\s\S]*?\}\s*\)/,
      'useWorkbenchPage must pass an onError to openNewTerminal that navigates to "/" ' +
        'so a failed createWorktree is not stranded on /wb/new without recovery.',
    );
  });
});

describe('openNewTerminal onError contract (regression #120)', () => {
  it('declares an onError parameter and invokes it in the catch path', () => {
    assert.match(
      useFeedbackWorktreeTerminalSrc,
      /onError\?:\s*\(err:\s*unknown\)\s*=>\s*void/,
      'openNewTerminal must accept an `onError?: (err: unknown) => void` callback',
    );
    assert.match(
      useFeedbackWorktreeTerminalSrc,
      /catch\s*\(\s*err\s*\)[\s\S]*?onError\?\.\(\s*err\s*\)/,
      'the catch block must invoke onError so callers can navigate the user ' +
        'out of the dead /wb/new state when createWorktree rejects.',
    );
  });

  it('cancelPreparing resets creatingRef so a superseding openNewTerminal can fire', () => {
    // Without this reset, the renderer can call cancelPreparing() (e.g. on
    // workbenchId or search.project transition) followed by a fresh
    // openNewTerminal and silently no-op behind the `if (creatingRef.current) return`
    // guard at the top of openNewTerminal — exact same dead-end shape as
    // the original bug. The bumped wtGenRef separately invalidates any
    // in-flight previous createWorktree response.
    assert.match(
      useFeedbackWorktreeTerminalSrc,
      /cancelPreparing\s*=\s*useCallback\(\s*\(\)\s*=>\s*\{[\s\S]*?creatingRef\.current\s*=\s*false[\s\S]*?\},/,
      'cancelPreparing must reset creatingRef.current = false alongside the ' +
        'wtGenRef bump so a superseding openNewTerminal call after it is not silently blocked.',
    );
  });
});

describe('WorkbenchPage drops the silent dead fallback (regression #120)', () => {
  it('contains no bare "Preparing workspace..." text fallback', () => {
    // The dead fallback at the pre-fix lines 25-29 was the user-visible end
    // of the bug. Removing it forces every isNewWorkbench render through
    // the WorkspacePreparing component (spinner + Cancel button) so the
    // user always has a visible action even if the URL hasn't transitioned
    // off /wb/new yet.
    assert.doesNotMatch(
      workbenchPageSrc,
      /Preparing workspace\.\.\./,
      'WorkbenchPage.tsx must not render a bare "Preparing workspace..." text — that ' +
        'silent fallback was the user-visible end of regression #120.',
    );
  });

  it('renders WorkspacePreparing for every isNewWorkbench branch', () => {
    // No inner branching on terminalPage.preparing — the URL is the only
    // truth, so any /wb/new render shows the spinner+Cancel UI.
    assert.match(
      workbenchPageSrc,
      /isNewWorkbench[\s\S]*?<WorkspacePreparing/,
      'isNewWorkbench branch must render WorkspacePreparing (the spinner+Cancel surface)',
    );
    assert.doesNotMatch(
      workbenchPageSrc,
      /terminal\.page\?\.preparing/,
      'WorkbenchPage must not gate the spinner on a stale terminalPage.preparing flag — ' +
        'that gate was what produced the dead-fallback fall-through.',
    );
  });
});

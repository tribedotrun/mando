// Regression: sidebar "New terminal" SquarePen previously called
// `handleNewTerminal` which `await`ed `createWorktree(...)` (driven by a
// network `git fetch origin`, ~1-5s) before any UI change. The fix flips
// it to navigate immediately to `/wb/new?project=X`, where the dormant
// `WorkspacePreparing` route renders a full-panel spinner on the same tick
// as the click. The actual `createWorktree` call moves to the page mount
// effect (`useWorkbenchPage.ts`), not this action.
//
// `newTerminalNavOptions` is the pure helper the action calls; locking its
// shape locks the route contract (`workbenchId === 'new'` is what
// `WorkbenchPage.isNewWorkbench` keys off).

import { describe, it } from 'node:test';
import assert from 'node:assert/strict';

import { newTerminalNavOptions } from '../service/sidebarNavHelpers.ts';

describe('newTerminalNavOptions', () => {
  it('returns immediate navigation to /wb/new with project search param', () => {
    const opts = newTerminalNavOptions('myproj');
    assert.deepEqual(opts, {
      to: '/wb/$workbenchId',
      params: { workbenchId: 'new' },
      search: { project: 'myproj' },
    });
  });

  it('uses the literal "new" workbenchId that WorkbenchPage keys WorkspacePreparing off', () => {
    const opts = newTerminalNavOptions('any');
    assert.equal(opts.params.workbenchId, 'new');
  });

  it('preserves the project name verbatim in the search param', () => {
    const opts = newTerminalNavOptions('proj with spaces');
    assert.equal(opts.search.project, 'proj with spaces');
  });
});

import { describe, it } from 'node:test';
import assert from 'node:assert/strict';

import { evidenceDeckPath } from '../service/evidenceHelpers.ts';

describe('evidenceDeckPath', () => {
  it('points at the deck inside the worktree evidence folder', () => {
    assert.equal(
      evidenceDeckPath('/Users/me/.mando/worktrees/mando-0829-1302'),
      '/Users/me/.mando/worktrees/mando-0829-1302/.ai/evidence/deck.html',
    );
  });

  it('does not double the separator when the worktree path has a trailing slash', () => {
    assert.equal(evidenceDeckPath('/tmp/wt/'), '/tmp/wt/.ai/evidence/deck.html');
  });
});

import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { buildTaskDetailTabs, resolveTaskDetailTab } from '../service/taskDetailTabs.ts';

describe('task detail deck tab', () => {
  it('shows Deck between PR and More only when the canonical deck exists', () => {
    assert.deepEqual(
      buildTaskDetailTabs(true).map((tab) => tab.key),
      ['feed', 'pr', 'deck', 'more'],
    );
    assert.deepEqual(
      buildTaskDetailTabs(false).map((tab) => tab.key),
      ['feed', 'pr', 'more'],
    );
  });

  it('falls back to Feed when a stale Deck URL no longer has a deck', () => {
    assert.equal(resolveTaskDetailTab('deck', buildTaskDetailTabs(false)), 'feed');
    assert.equal(resolveTaskDetailTab('deck', buildTaskDetailTabs(true)), 'deck');
  });
});

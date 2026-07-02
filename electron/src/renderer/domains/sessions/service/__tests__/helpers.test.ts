import assert from 'node:assert/strict';
import { describe, it } from 'node:test';

import { buildResumeCmd } from '../resumeCommand.ts';

describe('buildResumeCmd', () => {
  it('uses the OpenCode TUI resume command without requiring a run prompt', () => {
    assert.equal(buildResumeCmd('ses_open', 'opencode'), 'opencode --session ses_open');
    assert.equal(
      buildResumeCmd('ses_open', 'opencode', '/tmp/worktree'),
      'cd "/tmp/worktree" && opencode --session ses_open',
    );
  });
});

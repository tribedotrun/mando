// Fence the renderer-side evidence grouping contract.
//
// Bug #121: when a worker submitted before-state and after-state evidence in
// two `mando todo evidence` calls, the feed rendered them as two separate
// "Evidence" cards because each call creates its own `TaskArtifact`. This
// test fences the grouping helper so a future refactor that drops the merge
// or breaks the consecutive-only rule fails before reaching a UI surface.

import { describe, it } from 'node:test';
import assert from 'node:assert/strict';

import { groupEvidenceArtifacts, type RenderableFeedItem } from '../service/feedHelpers.ts';
import type { FeedItem, TaskArtifact } from '#renderer/global/types';

const evidenceArtifact = (
  id: number,
  createdAt: string,
  kind: 'before_fix' | 'after_fix' | null = null,
): TaskArtifact => ({
  id,
  task_id: 1,
  artifact_type: 'evidence',
  content: 'Evidence',
  media: [
    {
      index: 0,
      filename: `media-${id}.png`,
      ext: 'png',
      local_path: `artifacts/1/${id}-0.png`,
      remote_url: null,
      caption: null,
      kind,
    },
  ],
  created_at: createdAt,
});

const evidenceItem = (a: TaskArtifact): FeedItem => ({
  type: 'artifact',
  timestamp: a.created_at,
  data: a,
});

const workSummaryItem = (id: number, createdAt: string): FeedItem => ({
  type: 'artifact',
  timestamp: createdAt,
  data: {
    id,
    task_id: 1,
    artifact_type: 'work_summary',
    content: 'summary',
    media: [],
    created_at: createdAt,
  },
});

const timelineItem = (createdAt: string): FeedItem => ({
  type: 'timeline',
  timestamp: createdAt,
  data: {
    timestamp: createdAt,
    actor: 'captain',
    summary: 'worker completed',
    data: { event_type: 'worker_completed' },
  },
});

describe('groupEvidenceArtifacts', () => {
  it('merges two consecutive evidence artifacts into one evidence-group', () => {
    const before = evidenceArtifact(244, '2026-04-29T16:05:00Z', 'before_fix');
    const after = evidenceArtifact(245, '2026-04-29T16:05:30Z', 'after_fix');
    const result = groupEvidenceArtifacts([evidenceItem(before), evidenceItem(after)]);

    assert.equal(result.length, 1, 'two consecutive evidence rows collapse to one item');
    const group = result[0];
    assert.equal(group.type, 'evidence-group');
    if (group.type !== 'evidence-group') return;
    assert.deepEqual(
      group.artifacts.map((a) => a.id),
      [244, 245],
      'preserves artifact order',
    );
    assert.equal(group.timestamp, '2026-04-29T16:05:30Z', 'group timestamp equals latest member');
  });

  it('passes a single evidence artifact through as a plain artifact item', () => {
    const only = evidenceArtifact(244, '2026-04-29T16:05:00Z', 'before_fix');
    const result = groupEvidenceArtifacts([evidenceItem(only)]);

    assert.equal(result.length, 1);
    assert.equal(result[0].type, 'artifact');
  });

  it('breaks the group on a timeline event between two evidence rows', () => {
    const before = evidenceArtifact(244, '2026-04-29T16:05:00Z', 'before_fix');
    const after = evidenceArtifact(245, '2026-04-29T16:06:00Z', 'after_fix');
    const result = groupEvidenceArtifacts([
      evidenceItem(before),
      timelineItem('2026-04-29T16:05:30Z'),
      evidenceItem(after),
    ]);

    assert.equal(result.length, 3, 'no merging across timeline events');
    assert.deepEqual(
      result.map((r) => r.type),
      ['artifact', 'timeline', 'artifact'],
    );
  });

  it('does not merge work_summary artifacts with adjacent evidence', () => {
    const before = evidenceArtifact(244, '2026-04-29T16:05:00Z', 'before_fix');
    const summary = workSummaryItem(246, '2026-04-29T16:05:30Z');
    const after = evidenceArtifact(245, '2026-04-29T16:06:00Z', 'after_fix');
    const result = groupEvidenceArtifacts([evidenceItem(before), summary, evidenceItem(after)]);

    assert.equal(result.length, 3);
    assert.deepEqual(
      result.map((r) => r.type),
      ['artifact', 'artifact', 'artifact'],
    );
  });

  it('returns an empty array for an empty feed', () => {
    const result: RenderableFeedItem[] = groupEvidenceArtifacts([]);
    assert.equal(result.length, 0);
  });

  it('merges three consecutive evidence rows into a single group', () => {
    const a = evidenceArtifact(244, '2026-04-29T16:05:00Z', 'before_fix');
    const b = evidenceArtifact(245, '2026-04-29T16:05:30Z', 'after_fix');
    const c = evidenceArtifact(246, '2026-04-29T16:05:45Z', 'after_fix');
    const result = groupEvidenceArtifacts([evidenceItem(a), evidenceItem(b), evidenceItem(c)]);

    assert.equal(result.length, 1);
    if (result[0].type !== 'evidence-group') {
      assert.fail('expected evidence-group');
    }
    assert.deepEqual(
      result[0].artifacts.map((x) => x.id),
      [244, 245, 246],
    );
  });
});

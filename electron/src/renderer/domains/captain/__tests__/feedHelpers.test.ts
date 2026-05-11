// Fence the renderer-side evidence grouping contract.
//
// Bug #121 (PR #1038): a worker submitting before-state and after-state in two
// separate `mando todo evidence` calls rendered two disjoint "Evidence" cards.
// PR #1045 narrowed the merge after prod task 103 (5 untyped iterative
// re-shoots fused into one 15-file card). The current contract: the LAST two
// feed items merge into one `evidence-group` iff they form a labeled
// before/after pair — both evidence artifacts, one carrying `before_fix`,
// the other carrying `after_fix`. Anything else stays chronological. This
// suite guards both the pair-merge (don't drop it) and the no-fusing-of-
// untyped-or-non-pair rule (don't re-introduce eager grouping).

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

const evidenceUpdatedItem = (createdAt: string): FeedItem => ({
  type: 'timeline',
  timestamp: createdAt,
  data: {
    timestamp: createdAt,
    actor: 'captain',
    summary: 'evidence updated',
    data: { event_type: 'evidence_updated' },
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

  it('does not merge when the last two are same-kind (e.g. two after_fix)', () => {
    // Worker uploaded before_fix, then after_fix, then re-shot another
    // after_fix. The latest pair is (after, after) — not a labeled before/
    // after pair — so the trailing two stay chronological and the earlier
    // before stays as its own card.
    const a = evidenceArtifact(244, '2026-04-29T16:05:00Z', 'before_fix');
    const b = evidenceArtifact(245, '2026-04-29T16:05:30Z', 'after_fix');
    const c = evidenceArtifact(246, '2026-04-29T16:05:45Z', 'after_fix');
    const result = groupEvidenceArtifacts([evidenceItem(a), evidenceItem(b), evidenceItem(c)]);

    assert.equal(result.length, 3);
    assert.deepEqual(
      result.map((r) => r.type),
      ['artifact', 'artifact', 'artifact'],
    );
  });

  it('leaves untyped iterative re-shoots ungrouped (task 103 case)', () => {
    const items = [239, 240, 241, 242, 243].map((id, i) =>
      evidenceItem(evidenceArtifact(id, `2026-04-27T00:2${2 + i}:00Z`, null)),
    );
    const result = groupEvidenceArtifacts(items);

    assert.equal(result.length, 5, 'each untyped upload renders as its own chronological card');
    assert.deepEqual(
      result.map((r) => r.type),
      ['artifact', 'artifact', 'artifact', 'artifact', 'artifact'],
    );
  });

  it('leaves a trailing run ungrouped when it lacks one of the two kinds', () => {
    const a = evidenceArtifact(244, '2026-04-29T16:05:00Z', 'before_fix');
    const b = evidenceArtifact(245, '2026-04-29T16:05:30Z', 'before_fix');
    const result = groupEvidenceArtifacts([evidenceItem(a), evidenceItem(b)]);

    assert.equal(result.length, 2, 'two before_fix rows remain chronological without an after');
    assert.deepEqual(
      result.map((r) => r.type),
      ['artifact', 'artifact'],
    );
  });

  it('does not merge when the trailing run is untyped even if an earlier pair exists', () => {
    // The complement of the broader-merge case: a before/after pair
    // happened earlier, then a timeline event broke the run, then untyped
    // re-shoots followed. The trailing run is the untyped tail, which lacks
    // both kinds, so nothing merges and chronological order is preserved.
    const before = evidenceArtifact(244, '2026-04-29T16:00:00Z', 'before_fix');
    const after = evidenceArtifact(245, '2026-04-29T16:00:30Z', 'after_fix');
    const u1 = evidenceArtifact(250, '2026-04-29T16:05:00Z', null);
    const u2 = evidenceArtifact(251, '2026-04-29T16:06:00Z', null);
    const result = groupEvidenceArtifacts([
      evidenceItem(before),
      evidenceItem(after),
      timelineItem('2026-04-29T16:01:00Z'),
      evidenceItem(u1),
      evidenceItem(u2),
    ]);

    assert.equal(result.length, 5);
    assert.deepEqual(
      result.map((r) => r.type),
      ['artifact', 'artifact', 'timeline', 'artifact', 'artifact'],
    );
  });

  it('merges only the trailing labeled pair, leaving preceding untyped exploration chronological', () => {
    const u1 = evidenceArtifact(240, '2026-04-29T16:00:00Z', null);
    const u2 = evidenceArtifact(241, '2026-04-29T16:01:00Z', null);
    const before = evidenceArtifact(244, '2026-04-29T16:05:00Z', 'before_fix');
    const after = evidenceArtifact(245, '2026-04-29T16:05:30Z', 'after_fix');
    const result = groupEvidenceArtifacts([
      evidenceItem(u1),
      evidenceItem(u2),
      evidenceItem(before),
      evidenceItem(after),
    ]);

    assert.equal(result.length, 3, 'u1 + u2 stay individual; trailing pair merges');
    assert.deepEqual(
      result.map((r) => r.type),
      ['artifact', 'artifact', 'evidence-group'],
    );
    const grouped = result[2];
    if (grouped.type !== 'evidence-group') return;
    assert.deepEqual(
      grouped.artifacts.map((x) => x.id),
      [244, 245],
      'only the labeled before/after pair is grouped',
    );
  });

  it('merges (after_fix, before_fix) order pair just the same as (before, after)', () => {
    // Order doesn't determine kind — the worker may upload after first.
    const after = evidenceArtifact(244, '2026-04-29T16:05:00Z', 'after_fix');
    const before = evidenceArtifact(245, '2026-04-29T16:05:30Z', 'before_fix');
    const result = groupEvidenceArtifacts([evidenceItem(after), evidenceItem(before)]);

    assert.equal(result.length, 1);
    assert.equal(result[0].type, 'evidence-group');
  });

  it('walks past suppressed evidence_updated timeline events (production tail shape)', () => {
    // The daemon emits evidence_updated immediately after each evidence
    // upload, so it lands at the tail of feedItems. FeedBlocks filters it
    // out at render time, but the grouping helper sees it first. Without
    // dropping these, the last-pair check looks at (after, evidence_updated)
    // and never merges — meaning the pair-merge path wouldn't fire at all
    // in production.
    const before = evidenceArtifact(244, '2026-04-29T16:05:00Z', 'before_fix');
    const after = evidenceArtifact(245, '2026-04-29T16:05:30Z', 'after_fix');
    const result = groupEvidenceArtifacts([
      evidenceItem(before),
      evidenceItem(after),
      evidenceUpdatedItem('2026-04-29T16:05:31Z'),
    ]);

    assert.equal(result.length, 1, 'pair merges despite trailing suppressed event');
    assert.equal(result[0].type, 'evidence-group');
    if (result[0].type !== 'evidence-group') return;
    assert.deepEqual(
      result[0].artifacts.map((x) => x.id),
      [244, 245],
    );
  });

  it('drops suppressed timeline events from the renderable list even when no pair merges', () => {
    // task 103 in production: 5 untyped evidence rows, each followed by an
    // evidence_updated event. After filtering, the renderable list is the
    // 5 evidence cards — no merge, but the redundant timeline events also
    // don't appear (they would have rendered as null anyway).
    const u1 = evidenceArtifact(239, '2026-04-27T00:22:00Z', null);
    const u2 = evidenceArtifact(240, '2026-04-27T00:23:00Z', null);
    const result = groupEvidenceArtifacts([
      evidenceItem(u1),
      evidenceUpdatedItem('2026-04-27T00:22:01Z'),
      evidenceItem(u2),
      evidenceUpdatedItem('2026-04-27T00:23:01Z'),
    ]);

    assert.equal(result.length, 2);
    assert.deepEqual(
      result.map((r) => r.type),
      ['artifact', 'artifact'],
    );
  });
});

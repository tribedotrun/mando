import { describe, it } from 'node:test';
import assert from 'node:assert/strict';

import type { NotificationKind } from '#shared/notifications';
import { resolveNotificationClickTarget } from '../notificationClickRouting.ts';

const lookupHit =
  (workbenchId: number) =>
  (_taskId: number): number | null =>
    workbenchId;
const lookupMiss = (_taskId: number): number | null => null;

describe('resolveNotificationClickTarget', () => {
  describe('task kinds', () => {
    it('Escalated → workbench when lookup hits', () => {
      const kind: NotificationKind = { type: 'Escalated', item_id: '7', summary: null };
      assert.deepEqual(resolveNotificationClickTarget(kind, lookupHit(42)), {
        type: 'workbench',
        workbenchId: 42,
      });
    });

    it('Escalated → workbench-miss when lookup returns null', () => {
      const kind: NotificationKind = { type: 'Escalated', item_id: '7', summary: null };
      assert.deepEqual(resolveNotificationClickTarget(kind, lookupMiss), {
        type: 'workbench-miss',
        taskId: 7,
      });
    });

    it('NeedsClarification → workbench when lookup hits', () => {
      const kind: NotificationKind = {
        type: 'NeedsClarification',
        item_id: '12',
        questions: 'pick one',
      };
      assert.deepEqual(resolveNotificationClickTarget(kind, lookupHit(99)), {
        type: 'workbench',
        workbenchId: 99,
      });
    });

    it('NeedsClarification → workbench-miss when lookup returns null', () => {
      const kind: NotificationKind = {
        type: 'NeedsClarification',
        item_id: '12',
        questions: null,
      };
      assert.deepEqual(resolveNotificationClickTarget(kind, lookupMiss), {
        type: 'workbench-miss',
        taskId: 12,
      });
    });

    it('falls back to home when item_id is not numeric', () => {
      const kind: NotificationKind = { type: 'Escalated', item_id: 'not-a-number', summary: null };
      assert.deepEqual(resolveNotificationClickTarget(kind, lookupHit(99)), { type: 'home' });
    });
  });

  describe('scout kinds', () => {
    it('ScoutProcessed → scout target with scout_id (not item_id parse)', () => {
      const kind: NotificationKind = {
        type: 'ScoutProcessed',
        scout_id: 314,
        title: 't',
        relevance: 1,
        quality: 1,
        source_name: null,
        telegraph_url: null,
      };
      // Pre-fix bug: renderer parsed `item_id` as a task id and ran a workbench
      // lookup. The post-fix helper must reach into the typed kind for scout_id
      // and route to the scout reader unconditionally — even when the workbench
      // lookup would have hit by coincidence.
      assert.deepEqual(resolveNotificationClickTarget(kind, lookupHit(42)), {
        type: 'scout',
        scoutId: 314,
      });
    });

    it('ScoutProcessFailed → scout target with scout_id', () => {
      const kind: NotificationKind = {
        type: 'ScoutProcessFailed',
        scout_id: 271,
        url: 'https://example.com',
        error: 'boom',
      };
      assert.deepEqual(resolveNotificationClickTarget(kind, lookupMiss), {
        type: 'scout',
        scoutId: 271,
      });
    });
  });

  describe('other kinds', () => {
    it('RateLimited → home', () => {
      const kind: NotificationKind = {
        type: 'RateLimited',
        status: 'allowed',
        utilization: null,
        resets_at: null,
        rate_limit_type: null,
        overage_status: null,
        overage_resets_at: null,
        overage_disabled_reason: null,
      };
      assert.deepEqual(resolveNotificationClickTarget(kind, lookupHit(1)), { type: 'home' });
    });

    it('Generic → noop', () => {
      const kind: NotificationKind = { type: 'Generic' };
      assert.deepEqual(resolveNotificationClickTarget(kind, lookupHit(1)), { type: 'noop' });
    });
  });
});

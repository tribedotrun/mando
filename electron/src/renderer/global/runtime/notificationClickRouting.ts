import type { NotificationKind } from '#shared/notifications';

type NotificationClickTarget =
  | { type: 'workbench'; workbenchId: number }
  | { type: 'workbench-miss'; taskId: number }
  | { type: 'scout'; scoutId: number }
  | { type: 'home' }
  | { type: 'noop' };

export function resolveNotificationClickTarget(
  kind: NotificationKind,
  lookupTaskWorkbench: (taskId: number) => number | null,
): NotificationClickTarget {
  switch (kind.type) {
    case 'Escalated':
    case 'NeedsClarification': {
      const taskId = Number(kind.item_id);
      if (Number.isNaN(taskId)) return { type: 'home' };
      const workbenchId = lookupTaskWorkbench(taskId);
      return workbenchId !== null
        ? { type: 'workbench', workbenchId }
        : { type: 'workbench-miss', taskId };
    }
    case 'ScoutProcessed':
    case 'ScoutProcessFailed':
      return { type: 'scout', scoutId: kind.scout_id };
    case 'RateLimited':
      return { type: 'home' };
    case 'Generic':
      return { type: 'noop' };
  }
}

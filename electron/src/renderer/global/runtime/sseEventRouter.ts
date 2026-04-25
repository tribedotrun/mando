import type { QueryClient } from '@tanstack/react-query';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import { invalidateAllDaemonQueries } from '#renderer/global/repo/syncPolicy';
import {
  handleSessionsEvent,
  handleStatusEvent,
  patchScoutList,
  patchTaskList,
  patchWorkbenchList,
  seedFromSnapshot,
} from '#renderer/global/repo/sseCacheHelpers';
import { parseNotification } from '#renderer/global/service/notificationHelpers';
import type { SSEEvent } from '#renderer/global/types';
import log from '#renderer/global/service/logger';

interface SseEventRouterArgs {
  event: SSEEvent;
  queryClient: QueryClient;
  onError?: (message: string) => void;
  processDesktopNotification?: (event: SSEEvent) => void;
}

export function routeSseEvent({
  event,
  queryClient,
  onError,
  processDesktopNotification,
}: SseEventRouterArgs): void {
  switch (event.event) {
    case 'snapshot': {
      const counts = seedFromSnapshot(queryClient, event.data.data);
      log.debug('[sse] snapshot seeded caches', counts);
      break;
    }

    case 'snapshot_error': {
      const message = event.data.data.message;
      log.error('[sse] snapshot_error:', message);
      onError?.(message);
      invalidateAllDaemonQueries(queryClient, 'snapshot-error');
      break;
    }

    case 'tasks': {
      const payload = event.data.data;
      if (
        payload?.action === 'created' ||
        payload?.action === 'updated' ||
        payload?.action === 'deleted'
      ) {
        patchTaskList(queryClient, payload);
      } else {
        void queryClient.invalidateQueries({ queryKey: queryKeys.tasks.list() });
        void queryClient.invalidateQueries({ queryKey: queryKeys.workers.list() });
        void queryClient.invalidateQueries({ queryKey: queryKeys.stats.all });
      }
      break;
    }

    case 'scout': {
      const payload = event.data.data;
      if (
        payload?.action === 'created' ||
        payload?.action === 'updated' ||
        payload?.action === 'deleted'
      ) {
        patchScoutList(queryClient, payload);
      } else {
        void queryClient.invalidateQueries({ queryKey: queryKeys.scout.all });
      }
      break;
    }

    case 'workbenches': {
      const payload = event.data.data;
      if (
        payload?.action === 'created' ||
        payload?.action === 'updated' ||
        payload?.action === 'deleted'
      ) {
        patchWorkbenchList(queryClient, payload);
        void queryClient.invalidateQueries({
          queryKey: queryKeys.workbenches.all,
          predicate: (query) => query.queryKey.length > 2,
        });
      } else {
        void queryClient.invalidateQueries({ queryKey: queryKeys.workbenches.all });
      }
      break;
    }

    case 'status':
      handleStatusEvent(queryClient, event.data.data ?? null);
      break;

    case 'sessions':
      handleSessionsEvent(queryClient, event.data.data ?? null);
      break;

    case 'notification': {
      const payload = parseNotification(event);
      if (payload) {
        if (payload.kind?.type === 'RateLimited') {
          log.info('[sse] rate-limit notification in non-feedback router', payload.message);
        }
      } else if (event.data.data) {
        log.warn('[sse] unexpected notification shape:', event.data);
      }
      break;
    }

    case 'research': {
      const payload = event.data.data;
      if (payload) {
        if (payload.action === 'completed') {
          void queryClient.invalidateQueries({ queryKey: queryKeys.scout.all });
        } else if (payload.action === 'failed') {
          void queryClient.invalidateQueries({ queryKey: queryKeys.scout.research() });
        } else if (payload.action === 'started' || payload.action === 'progress') {
          void queryClient.invalidateQueries({ queryKey: queryKeys.scout.research() });
        }
      }
      break;
    }

    case 'artifacts': {
      const payload = event.data.data;
      if (payload?.task_id) {
        void queryClient.invalidateQueries({ queryKey: queryKeys.tasks.feed(payload.task_id) });
        void queryClient.invalidateQueries({
          queryKey: queryKeys.tasks.artifacts(payload.task_id),
        });
      }
      break;
    }

    case 'config':
      void queryClient.invalidateQueries({ queryKey: queryKeys.config.all });
      break;

    case 'credentials':
      void queryClient.invalidateQueries({ queryKey: queryKeys.credentials.all });
      break;

    case 'resync':
      log.warn('[sse] resync -- invalidating all caches');
      invalidateAllDaemonQueries(queryClient, 'explicit-resync');
      break;

    default: {
      const unexpected: never = event;
      log.error('[sse] unexpected daemon event', unexpected);
      invalidateAllDaemonQueries(queryClient, 'unexpected-event');
      break;
    }
  }

  processDesktopNotification?.(event);
}

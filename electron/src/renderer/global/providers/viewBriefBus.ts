/**
 * Pub/sub for "view task brief" requests. Replaces the
 * `mando:view-task-brief` DOM CustomEvent that previously bridged the app
 * header (source) to the task-detail view (target). Using a typed
 * subscribe/unsubscribe pair keeps the dependency legible and auditable.
 *
 * Source: `app/AppHeaderTask.tsx` calls `requestViewTaskBrief()`.
 * Target: `domains/captain/runtime/useTaskDetailView.ts` subscribes and
 *         opens the context modal on each call.
 */
// eslint-disable-next-line no-restricted-imports -- logger is cross-cutting infrastructure
import log from '#renderer/global/service/logger';

type Listener = () => void;

const listeners = new Set<Listener>();

export function requestViewTaskBrief(): void {
  for (const fn of listeners) {
    try {
      fn();
    } catch (err) {
      log.warn('[viewBriefBus] subscriber threw during requestViewTaskBrief:', err);
    }
  }
}

export function subscribeViewTaskBrief(fn: Listener): () => void {
  listeners.add(fn);
  return () => {
    listeners.delete(fn);
  };
}

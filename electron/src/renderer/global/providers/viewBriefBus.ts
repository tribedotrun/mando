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

function createViewBriefBus() {
  const listeners = new Set<Listener>();

  return {
    request(): void {
      for (const fn of listeners) {
        try {
          fn();
        } catch (err) {
          log.warn('[viewBriefBus] subscriber threw during requestViewTaskBrief:', err);
        }
      }
    },
    subscribe(fn: Listener): () => void {
      listeners.add(fn);
      return () => {
        listeners.delete(fn);
      };
    },
  };
}

const viewBriefBus = createViewBriefBus();

export function requestViewTaskBrief(): void {
  viewBriefBus.request();
}

export function subscribeViewTaskBrief(fn: Listener): () => void {
  return viewBriefBus.subscribe(fn);
}

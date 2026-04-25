/**
 * Pub/sub for observability degradation events. Replaces the
 * `mando:obs-degraded` DOM CustomEvent so feature code can react to obs
 * degradation through a typed contract instead of a stringly-typed event bus.
 *
 * `reportObsDegraded()` is called from the http provider when the client-log
 * flush exceeds its retry budget or when the SSE stream first emits a parse
 * failure for a session. `subscribeObsDegraded(fn)` returns an unsubscribe
 * function and is called from the data provider to surface a toast.
 */
// eslint-disable-next-line no-restricted-imports -- logger is cross-cutting infrastructure
import log from '#renderer/global/service/logger';

type Listener = () => void;

function createObsHealthBus() {
  const listeners = new Set<Listener>();

  return {
    report(): void {
      for (const fn of listeners) {
        try {
          fn();
        } catch (err) {
          // DOM dispatchEvent (which this replaces) runs every registered
          // listener regardless of sibling failures; preserve that semantic
          // so one throwing subscriber does not cancel the rest.
          log.warn('[obsHealth] subscriber threw during reportObsDegraded:', err);
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

const obsHealthBus = createObsHealthBus();

export function reportObsDegraded(): void {
  obsHealthBus.report();
}

export function subscribeObsDegraded(fn: Listener): () => void {
  return obsHealthBus.subscribe(fn);
}

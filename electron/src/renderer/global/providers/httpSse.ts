import type { SSEConnectionStatus, SSEEvent } from '#renderer/global/types';
// eslint-disable-next-line no-restricted-imports -- logger is cross-cutting infrastructure
import log from '#renderer/global/service/logger';
import {
  resolveRoutePath,
  type RouteEvent,
  type SseRouteOptions,
} from '#shared/daemon-contract/runtime';
import type { GetSseRouteWithEventKey } from '#shared/daemon-contract/routes';
import { eventSchemas } from '#shared/daemon-contract/schemas';
import type { ZodType } from 'zod';
import { parseSseMessage } from '#result';
import { buildUrl } from '#renderer/global/providers/httpBase';
import { queueError } from '#renderer/global/providers/httpObsQueue';
import { reportObsDegraded } from '#renderer/global/providers/obsHealth';

export function openSseRoute<K extends GetSseRouteWithEventKey>(
  key: K,
  options?: SseRouteOptions<K>,
): EventSource {
  return new EventSource(buildUrl(resolveRoutePath(key, options)));
}

export function connectSSE(
  onEvent: (event: SSEEvent) => void,
  onStatusChange?: (status: SSEConnectionStatus) => void,
): EventSource {
  const source = openSseRoute('getEvents');

  let lastStatus: SSEConnectionStatus | null = null;
  const emitStatus = (status: SSEConnectionStatus) => {
    if (status === lastStatus) return;
    lastStatus = status;
    onStatusChange?.(status);
  };

  emitStatus('connecting');
  source.onopen = () => emitStatus('connected');

  let consecutiveParseFailures = 0;
  let degradedEmittedForStream = false;
  const sseSchema = (eventSchemas as Record<string, ZodType<unknown> | undefined>).getEvents;

  source.onmessage = (message) => {
    const result = parseSseMessage(message.data as unknown, sseSchema);
    if (result.failure) {
      consecutiveParseFailures++;
      queueError('error', 'SSE parse_failed', result.failure);
      if (!degradedEmittedForStream) {
        degradedEmittedForStream = true;
        reportObsDegraded();
      }
      if (consecutiveParseFailures === 5) {
        log.error('[SSE] 5 consecutive parse failures, data stream may be corrupt');
        emitStatus('disconnected');
      }
      return;
    }

    consecutiveParseFailures = 0;
    const data = result.data as RouteEvent<'getEvents'>;

    if (data.event === 'snapshot_error') {
      const payload = data.data.data;
      const reason =
        payload && typeof payload === 'object' && 'message' in payload
          ? String((payload as { message: unknown }).message)
          : 'server failed to build snapshot';
      log.error('[SSE] snapshot_error event from server:', reason);
      queueError('error', `SSE snapshot failed: ${reason}`);
      emitStatus('disconnected');
    } else if (data.event === 'resync') {
      log.warn('[SSE] broadcast lagged, client must reload snapshot');
    }

    onEvent(data);
  };

  source.onerror = () => {
    log.warn('[SSE] connection error — will auto-reconnect');
    emitStatus('disconnected');
  };

  return source;
}
